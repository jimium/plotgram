# Showcase 与基线分层重构方案

> 日期：2026-07-20  
> 状态：方案（样例稿已落在 [`casev2/`](./casev2/)，门禁脚本未改）  
> 相关：[`AGENTS.md`](../../../AGENTS.md) §4–§7 · [`布局与路由核心手册`](../../总结经验/布局与路由核心手册-2026-07.md) §1 · [`benchmark-data/README.md`](../../../benchmark-data/README.md) · 样例评审稿 [`casev2/README.md`](./casev2/README.md)

## 0. 一句话

**文件名即角色**（`smoke.` / `product.` / `demo.` / `stress.` / `mech.`）；gates 只挑子集进门禁。  
产品图硬好看；压力图硬正确、软美观；机制探针验契约、不追观感。日常「无退化」默认只引用 product-gate。

---

## 1. 问题诊断

### 1.1 现状：`s./n./c.` 把两件事揉成了一轴

`showcase/` 按复杂度前缀组织：

| 前缀 | 级别 | 节点数（约定） | 原意图 |
|------|------|----------------|--------|
| `s.` | simple | ≤4 | 基础语法 |
| `n.` | normal | 5–10 | 日常业务 |
| `c.` | complex | 10+ | 复杂业务 / 压力 |

但 `c.` 下实际混了三类性质不同的文件：

| 性质 | 例子 | 工程期望 |
|------|------|----------|
| **业务复杂**（用户会画） | `c.cloud-native`、`c.ecommerce-platform`、`n.user-auth` | 必须好看 |
| **对外演示大图** | `c.supply-chain-control-tower`、`c.payment-clearing-platform` | 好看优先，可有限债 |
| **人造压力探针** | `c.layout-stress-*`（5 张） | 可妥协美观 |

另有一类**机制探针**（现挂在 `n.`）：`n.constrain-cross-group`、`n.constrain-sink`，以及 `benchmark-data/congestion-set.txt`——目的是验规则，不是刷美观分。

规模统计（约）：`s.` 13 · `n.` 23 · `c.` 49。`c.` 过大，分类已失真。

### 1.2 基线把异质样例绑在同一质量棘轮上

| 表面 | 事实 |
|------|------|
| 画廊给 `layout-stress` 打「布局压力」标签 | **仅 UI**，不进门禁策略 |
| `collinear-regression-set.txt` 共 10 张 | 含 2 张 stress，与业务大图**同等**走 `compare-collinear.sh` |
| 正确性轨 / 质量轨 | 只按指标分轨，**不按样例角色分轨** |
| `plotgram-eval` 的 `generate_baseline` / `compare_with_baseline` | 遍历 showcase `.pgm`，**无角色加权 / 豁免** |

共线集当前成员（stress 已高亮）：

```text
architecture/c.layout-stress-nested.pgm      ← stress
architecture/c.cloud-native.pgm
architecture/c.k8s-multi-cluster-federation.pgm
architecture/c.k8s-multi-namespace-overview.pgm
architecture/c.ecommerce-platform.pgm
architecture/c.hybrid-cloud-dr-topology.pgm
flowchart/c.layout-stress-dag.pgm            ← stress
flowchart/c.aml-case-investigation.pgm
architecture/c.k8s-tenant-isolation.pgm
architecture/c.k8s-platform-stack.pgm
```

### 1.3 扭曲激励

假设一次改动：

- 日常 / 业务图平均质量 ↑  
- stress 的 `tight_sev` / `degraded_count` ↑  

在现行规则下，质量轨仍可能 FAIL。为压住 stress 数字，算法易被推向更重启发、更长路径；这些路径对正常图往往冗余甚至负优化。

**根因不是「有压力样例」，而是「门禁把压力样例当成了与产品图同等的优化目标」。**

### 1.4 旧稿不足（本稿相对初版的修正）

| 旧稿 | 问题 | 本稿 |
|------|------|------|
| 全体 `s./n./c.`「严格不退化」 | `c.` 里大量 demo 大图不应全部硬棘轮 | **精选 product-gate**；demo 观测或可债 |
| Step 1 先重命名 `x.` | 引用面大（单测、基线 JSON、画廊、文档），收益滞后 | **先改清单与 compare 策略**，命名后置 |
| 只拆 stress vs 业务 | 漏掉 mech / demo | 五角色 |
| stress「不挡门禁」一刀切 | 丢掉探针价值（穿模、确定性仍应硬） | **正确性硬、质量软** |
| 按前缀自动分轨 | 旧 `s/n/c` 前缀 ≠ 工程角色 | **文件名即角色**；gates 精选子集 |
| 规模前缀 + overrides + gates | 三套机制重叠 | 角色进文件名；gates 只挑选；manifest 无 overrides |

---

## 2. 目标模型：文件名 = 角色

```text
{type}/{role}.{slug}.pgm
  role ∈ smoke | product | demo | stress | mech
```

UI / 工具：`filename.split('.')[0]` → role。旧 `s./n./c./x.` **退役**（规模不再占前缀）。

### 2.1 角色定义

| 角色 | 文件名前缀 | 观感 | 门禁 |
|------|------------|------|------|
| **smoke** | `smoke.` | 必须干净 | 正确性硬；质量可进轻量集 |
| **product** | `product.` | **必须好看** | **精选进质量硬棘轮** |
| **demo** | `demo.` | 好看优先 | 观测 / 可债 |
| **stress** | `stress.` | **可妥协** | 正确性硬；质量观测 |
| **mech** | `mech.` | 不追美观 | 机制断言 / 专项集 |

判定口诀：

- **product**：用户下周就可能画成这样  
- **demo**：给销售与官网看的大图  
- **stress**：故意造来打爆布局/路由的  
- **mech**：为钉死一条规则而写的最小反例  

### 2.2 与 gates 的关系

- **角色**：看文件名（全库每张图都有）。  
- **gates**：从全库**挑选子集**（谁进硬门 / 谁当探针）。  
- 允许 mech-set 列出 `product.user-auth.pgm`（双用途）：UI 角色仍是 product，清单只表示「也跑机制断言」。  
- **不需要** manifest `overrides`。样例见 [`casev2/`](./casev2/)。

---

## 3. 门禁分层

### 3.1 三套样例集（文件可仍在 showcase，清单分开）

| 集 | 内容 | 正确性轨 | 质量轨（sev / degraded / lint 摘要） | 宣称「无退化」时 |
|----|------|----------|--------------------------------------|------------------|
| **A · product-gate** | 精选 product（+ 少量 smoke） | 硬 | **硬** | **必须引用本集** |
| **B · stress-probe** | 全部 stress | 硬（穿模 / det 等） | **观测**；可用 `--allow-stress-debt` | 须标明「探针集」 |
| **C · mech-set** | constrain / congestion 等 | 机制断言 | 可不进 sev 棘轮 | 单独叙述 |

可选第四集 **D · demo-observe**：大图全量跑报告，不进硬质量门。

原则：

> **压力图证明系统没坏；产品图证明用户会满意。**

### 3.2 指标分轨（在角色之上）

对**每一张**进入快照的样例，仍保留现有双轨语义，但按角色决定「质量 FAIL 是否挡合并」：

| 轨 | 典型指标 | product | demo | stress | mech |
|----|----------|---------|------|--------|------|
| 正确性（硬） | `edge_crosses_group_interior` 不升；`det=true`；节点穿模类 | FAIL | FAIL | FAIL | 按机制定义 |
| 质量（默认真） | `exact_sev` / `tight_sev`；through / trunk / err；`ortho.degraded_count`；perf | FAIL | WARN / 可债 | WARN / `--allow-stress-debt` | 通常不门禁 |
| 观测 | `allowed_share_len` 等 | 同现网 | 同现网 | 同现网 | — |

说明：

- stress **绝不是**「完全不管」——正确性升仍挡合并（探针价值）。  
- 质量在 stress 上允许有界退化；超界标 WARN，提示人工抽检，默认不 `exit 1`。  
- **禁止**为压 stress 质量数字加图名分支（`AGENTS.md` / 手册 ★）。

### 3.3 与创新模式（`AGENTS.md` §7）对齐

| 模式 | product-gate | stress-probe |
|------|--------------|--------------|
| 日常修复（棘轮） | 不劣化 | 质量可债；正确性不劣化 |
| 算法级重写（帕累托） | 目标维度优先看本集 | 允许更大临时质量退化，退出时显式抬基线 + 列残余 |

抬基线 `note` 强制带角色，例如：

```text
raise product: …原因…；残余: c.foo
raise stress (expected): …探针可接受…；残余: x.layout-stress-nested
```

---

## 4. Showcase 内容与分类

### 4.1 落地方式（casev2 已按此做）

1. **重命名**：`{role}.{slug}.pgm`（见 [`casev2/`](./casev2/)、[`rename-map.txt`](./casev2/gates/rename-map.txt)）。  
2. **门禁清单**：`gates/product|stress|mech|demo-*.txt` → 迁入 `benchmark-data/`。  
3. **manifest**：仅 roles 词典 + gates 指针（无 overrides）。  
4. **画廊**：按文件名前缀 badge；默认折叠 `stress.` / `mech.`。  

替换现网 `showcase/` 时同步：`README`、`index.html`、基线 JSON、单测路径等。

### 4.2 Product-gate 选样原则

- **小而稳**：建议 **12–20 张**，覆盖各图类型，而非「所有非 stress」。  
- 每类型至少 1 张日常 `n.`（或等价 product）。  
- 架构：1–2 张真实多 group（如 `cloud-native`、典型微服务），**不要**堆满全部 K8s 全景。  
- 流程：分支+回环、泳道（若有）各至多 1 张代表。  
- **明确排除**：全部 `layout-stress-*`；「为刷指标造的」超密图；过胖 demo 默认不进 A 集。  
- 筛法口令：**用户会不会画成这样**，不按节点数筛。

### 4.3 候选名单（已落到 casev2）

权威清单见评审稿目录（路径相对 `casev2/`）：

| 角色集 | 文件 |
|--------|------|
| product-gate | [`casev2/gates/product-regression-set.txt`](./casev2/gates/product-regression-set.txt)（约 18 张） |
| stress-probe | [`casev2/gates/stress-probe-set.txt`](./casev2/gates/stress-probe-set.txt)（6 张 `x.*`） |
| mech-set | [`casev2/gates/mech-set.txt`](./casev2/gates/mech-set.txt) |
| demo-observe | [`casev2/gates/demo-observe-set.txt`](./casev2/gates/demo-observe-set.txt) |
| 角色总表 | [`casev2/manifest.yaml`](./casev2/manifest.yaml) |

替换 `showcase/` 后，将上述清单迁到 `benchmark-data/`，路径改为 `showcase/...`。

mech 与 product 允许交集（同一文件两种用途时：机制用断言，美观用 product-gate 指标——报告里分栏，避免双重惩罚叙事混乱）。

### 4.4 画廊 UX

- 默认 tab / 筛选：**product + demo**（对外叙事）。  
- 「工程探针」折叠区：stress + mech。  
- 标签：保留场景 tag（K8s、金融…）；角色用独立 badge（`product` / `stress`…），不要只靠文件名启发式。

---

## 5. 工具与报告改动

### 5.1 `compare-collinear.sh`（及同类 compare）

- 输入可带 role（来自清单分段注释，或 JSON 内 `role` 字段）。  
- 输出示例：

```text
── product-gate（N 文件）──  正确性 ✓  质量 ✓
── stress-probe（M 文件）──  正确性 ✓  质量 ⚠（观测，不挡）
── demo-observe …          仅摘要
```

- 新增 flag 建议：`--allow-stress-debt`（仅放松 stress 质量轨）；保留现有 `--allow-quality-debt` / `--allow-node-fp`。  
- **默认日常命令**只对 product-gate 做硬 fail；全量探针可另跑一条 CI job（soft）。

### 5.2 `plotgram-eval` baseline

- `generate_baseline`：仍可扫全量，但样本记录写入 `role`。  
- `compare_with_baseline`：按角色决定质量是否计入 fail；**全局平均分排除 stress**（或分栏：product 均值 / stress 均值）。  
- 报告增加按角色汇总行（替代纯扁平「有回归的文件列表」作为主叙事）。

### 5.3 快照清单

建议演进：

| 文件 | 用途 |
|------|------|
| `product-regression-set.txt` | 日常共线 / 质量硬门禁 |
| `stress-probe-set.txt` | 正确性硬 + 质量观测 |
| `collinear-regression-set.txt` | 过渡期：改为「product ∪ 抽样子」或废弃并改文档指针 |
| `phase0-regression-set.txt` | 与 product 对齐或标明子集 |
| `congestion-set.txt` | 保留为 mech |

过渡期可将现 collinear 集内 2 张 stress **降级为 probe 段**（同文件加注释），避免一次拆文件过大。

---

## 6. 实施优先级

| 步骤 | 内容 | 风险 | 可独立合入 |
|------|------|------|------------|
| **P0** | 拆 / 标注清单：stress 出硬质量门；新建 product-regression-set；compare 按段分轨 | 低 | 是 |
| **P1** | 报告按角色汇总；宣称无退化默认只引用 product | 低 | 是 |
| **P2** | `showcase/manifest.yaml`（或文件头 `@role`）；画廊按角色过滤 | 低 | 是 |
| **P3** | 用 casev2 角色前缀替换现网 `showcase/` + 全仓引用 | 中（路径） | 是 |
| **P4** | eval `baseline.rs` 角色感知；CI 分 job（product 硬 / stress soft） | 中 | 是 |

**不要**把 P3 当第一步：门禁行为纠正后，命名只是可读性与防再混入。

验收（DoD）：

1. 仅 stress 质量变差、product 持平或变好 → 日常门禁 **绿**（stress 最多 WARN）。  
2. stress 正确性变差 → 仍 **红**。  
3. 文档 / README / 手册一句口径统一：「无退化 ≡ product-gate」。  
4. 无图名特判；抬基线 note 带角色。

---

## 7. 设计原则（执行清单）

1. **正常图是王道**：product-gate 渲染质量是核心目标。  
2. **压力图是探针**：正确性硬、质量软；可记录债，不驱动算法复杂度膨胀。  
3. **Demo 不绑架日常棘轮**：大图好看靠抽检与观测基线，不靠与 stress/product 混权。  
4. **分层可观测**：随时能回答「产品图在变好吗」和「压力图在烂多少」。  
5. **渐进**：P0 即可纠激励；重命名与 eval 深度改造后置。  
6. **禁图名特判**：stress 差 → 记债或抬探针基线，不为单图开分支。  
7. **卫生红线不豁免**：确定性、`cargo run -p plotgram-cli` 验真、WASM 禁 `std::time` 等仍适用。

---

## 8. 刻意不做

- 不为每张图维护独立阈值表或特判路径。  
- 不删除 stress（探针有价值）。  
- 不用「全 showcase 平均严重度」作为唯一分数。  
- 不把「全体非 x 前缀」自动当成硬门禁——**精选**才是门禁。

---

## 9. 修订记录

| 日期 | 说明 |
|------|------|
| 2026-07-20 | 初版：`x.` 前缀 + 全体 s/n/c 硬门禁 + 先改名 |
| 2026-07-20 | 修正：角色两轴；product 精选门禁；stress 正确性硬/质量软；mech/demo；实施顺序改为清单优先 |
| 2026-07-20 | [`casev2/`](./casev2/) 样例稿对齐：改 README 门禁口径、补 gates/manifest、修 refund 回环与 mindmap stress |
| 2026-07-20 | 简化：废弃 `s/n/c/x` 与 overrides；**文件名即角色**（`smoke.|product.|demo.|stress.|mech.`），gates 只挑选 |
