# Hierarchical · PartitionGrid 实现方案（PG-0–PG-4）

> 父页：[architecture](../architecture.md) §8 · [roadmap](../roadmap.md) §6 E / M5  
> 契约真源：[shared/partition](../../shared/partition.md) · [ADR-008](../../../adr/008-partition-grid.md)  
> 坐标映射：[coordinate-and-demand](coordinate-and-demand.md) §3  
> 组合相：[composition](composition.md) §1 / §6（连续块与 group 同构）  
> 与组边界：[group-frame-d2](group-frame-d2.md)（group ≠ 泳道；禁止冒充）  
> 写权尺子：[write-authority](../../write-authority.md)  
> 状态：**现行实现方案**（可执行推进）；产品选型「泳道/矩阵 → PartitionGrid」

本文把 **PartitionGrid 引擎消费**切成 **PG-0 → PG-4** 可串行切片，钉死与 group / StrongMacro / Channel `lane` 的边界、写权、管线序与验收。

---

## 1. 一句话

**PartitionGrid** = 正交于流向的**全局**列/行格子：节点占 `partition_cell`；列（TB 下）成层内连续块 + cross-axis band；行成全局 rank 区间 + main-axis band。层跨列对齐，**禁止**每泳道独立分层，**禁止**用 `group` Horizontal 冒充。

它解决泳道/矩阵产品能力；**不是** group 包含树，也**不是** Channel 走廊 track（勿与 `lane` 混淆）。

---

## 2. 资料地图

| 文档 / 代码 | 角色 |
|-------------|------|
| [ADR-008](../../../adr/008-partition-grid.md) | 一等 IR 决策；非目标 |
| [shared/partition.md](../../shared/partition.md) | 三分语义 + 引擎约束契约（现行） |
| [coordinate-and-demand §3](coordinate-and-demand.md) | Orientation → main/cross band 映射 |
| [architecture §8.3](../architecture.md) | 连续块：组与列同构机制 |
| `plotgram_model::partition` | `PartitionGrid` / `PartitionCell` / `validate_graph_partition` ✅ |
| `plotgram-parse` | `partition { }` + `cell_col`/`cell_row` lift ✅（端到端测已有） |
| `hierarchical/compose/graph_index.rs` | **缺口**：建 `RealGraph` 时丢掉 cell（未进 IR） |
| `hierarchical/compose/boundary.rs` | 组边界 dummy 先例（列块可同构） |
| `apps/showcase/hierarchical/partition/` | 仅 `.gitkeep` — 待加 showcase |
| dsl-spec | `partition` / `cell_*` 语法 **active**；Hier 消费仍标 planned（落地后改） |

---

## 3. 与邻近工作的边界（勿混）

| 工作 | 回答的问题 | 与 PartitionGrid |
|------|------------|------------------|
| **group / Weak·Strong** | 嵌套包含、组框 | **正交叠加**：节点可同时有 group_path 与 cell；禁止 group⇒列 |
| **D₂ 组框** | Weak 框真源 | 列/行 **band** 另有写者；框与 band 可对齐但不混真源 |
| **Channel lane/track** | 走廊分轨 | 同名异义；IR/API 禁止把 partition 叫 lane |
| **StrongMacro** | 架构舞台 | 可与 partition 同图；macro 不得发明 cell |
| **swimlane/table 糖** | 语法糖 | **首期不做**；若后做必须展开为显式 grid+cell |

产品口诀：嵌套子系统 → group / Strong；**泳道/矩阵 → PartitionGrid**（本文）。

---

## 4. 现状（开 PG-0 前）

### 4.1 已具备

- Model + `validate_graph_partition`（无 grid 写 cell / 未知轴 / id 冲突）  
- Parse：`partition { column|row }`、`cell_col`/`cell_row` lift（`parse` 端到端测）  
- Orientation 双射与 canonical TB 内核  
- GroupBoundary 连续块 + 跨层对齐（列块可复用机制，政策不同）  
- DemandBoard / VPSC / hier_eval 门禁骨架  

### 4.2 缺口

| 缺口 | 影响 |
|------|------|
| `RealGraph` 无 cell / 无 grid 引用 | Hier **完全不消费** partition |
| 无列连续块 / 无 band 变量 | 泳道观感靠人工摆节点，跨列 rank 不对齐无保证 |
| 无行→rank 区间 | 矩阵主轴带未建模 |
| showcase `partition/` 空 | 无验收图 |
| Render 分区标题/带背景 | 可后置；不发明 cell |

---

## 5. 目标架构

### 5.1 轴映射（钉死）

作者声明的 `columns` / `rows` 永远是**物理**轴（ADR-008 / coordinate-and-demand §3）：

| Orientation | cross-axis band（层内连续块） | main-axis band（层区间） |
|-------------|------------------------------|--------------------------|
| TB / BT | **columns** | **rows**（若有） |
| LR / RL | **rows** | **columns**（若有） |

反向流（BT/RL）只反转 rank 映射，**不**反转作者 axis 声明序。Metric 输出仍保证 column 物理左→右、row 上→下。

### 5.2 管线形状

```text
Graph.partition + Node.partition_cell
        │
        ▼
┌─ PG-A  输入投影 ─────────────────────────────────────────────┐
│  validate_graph_partition（已有）→ RealGraph 携带 cell        │
│  读 grid 引用进 Compose（只读作者事实）                       │
└──────────────────────────────────────────────────────────────┘
        │
        ▼
┌─ PG-B  Compose（离散）───────────────────────────────────────┐
│  Ranking：全局一层；若有 main-axis cell → rank 落入行区间     │
│  Ordering：cross-axis 同 cell 节点 → 连续块（PartitionBoundary │
│            或复用边界 dummy 机制，政策 = 列/行 id）            │
│  未指派 cell：自由区（默认允许；可 param 收紧）               │
└──────────────────────────────────────────────────────────────┘
        │
        ▼
┌─ PG-C  Metric（像素）────────────────────────────────────────┐
│  cross：列/行 band L/R（或 T/B）变量 + 含 cell 节点 + 空轴    │
│         min size / 标题 Demand                               │
│  main：行 band 与 layer 区间一致；Demand 进 LayerGap          │
│  与 group 框约束共存；单写者 = Metric                         │
└──────────────────────────────────────────────────────────────┘
        │
        ▼
Channel / Ink（可读 band 作障碍可选）→ LayoutOutput
  + partition_band_coords（或等价调试/渲染字段）
```

### 5.3 写权表

| 自由度 | 写者 | 禁止 |
|--------|------|------|
| 轴声明序 / id / label | DSL → model | 引擎改名或重排轴 |
| `partition_cell` | DSL lift | Metric/Ink 改 cell |
| 层内列序连续块 | Compose（Partition 连续块） | Ink 挪列；每列独立 FAS/rank |
| 列/行像素 band | Metric | finalize 发明 band；group 框冒充列 |
| 组框 | 既有 Weak Metric / Strong MacroBlock | 与 band 双真源互相覆盖 |
| Channel track | Channel | 把 partition 当 lane |

### 5.4 明确禁止

1. 每泳道独立 Network Simplex / 独立 rank。  
2. `group` Horizontal / StrongMacro 冒充 PartitionGrid。  
3. Channel / Ink 模块内 `diagram_type` 或「泳道」特判。  
4. 用整数下标替代 axis id（ADR 已否）。  
5. 首期 `swimlane`/`table` 糖、`group { lane: true }` 自动成列。

---

## 6. IR 草图

### 6.1 Compose 侧

```text
RealGraph {
  … existing …
  // PG-0: parallel dense arrays (declaration index)
  partition_column: Vec<Option<String>>   // axis id or None
  partition_row:    Vec<Option<String>>
}
// or: partition_cell: Vec<Option<PartitionCell>>

PlanGraph / Elem:
  // PartitionBoundary { axis_id, rank, side }  — 可选；或复用
  // GroupBoundary 机制但 key 用 PartitionColumn{…}
```

建议：**优先**新增 `PartitionBoundary`（或通用 `BandBoundary { kind: Group|PartitionColumn, id, …}`），避免 GroupBoundary 语义过载；机制抄 `boundary.rs`。

### 6.2 Metric / 输出

```text
DemandKey::PartitionBandMinSize { axis: Column|Row, id }
PartitionBandCoords {   // canonical TB，再 OrientationOut
  columns: [(id, x0, x1)]  // 声明序
  rows:    [(id, y0, y1)]
}

LayoutOutput / LayoutResult:
  // 首期可只进 debug / HierarchicalObs；渲染带后置
  // 或 LayoutResult.partition_bands（若产品要画泳道底色）
```

未指派 cell 的节点：不进任何 band 含约束；可落在「自由区」（band 外或缝中）——默认宽松（ADR-008 §6）。

### 6.3 与 group 同图

- 同层：先满足 partition 连续块序，再套 group clamp（或联合构造 precedence；成环 → `InfeasibleConstraint`）。  
- 坐标：node ⊆ group frame **且** node ⊆ partition band（若有 cell）。  
- StrongMacro：intra 局部图仍读 cell；expand 后全局 band 一次写（禁止 Ink 区分）。

---

## 7. 推进步骤

### PG-0 · 输入接线与验收图壳 ≈ 0.5–1 周

**目标**：Hier 能**看见** partition；无 grid 时 bit-identical；有 grid 尚未排泳道也不崩溃。

- [ ] `build_real_graph`：从 `Graph` 拷贝 `partition_cell`（dense 并行数组）  
- [ ] layout 入口：`validate_graph_partition` → `InvalidInput` / `LayoutError`（勿静默）  
- [ ] 无 `partition` / 全无 cell：现有 fixture **零几何变化**  
- [ ] showcase：`apps/showcase/hierarchical/partition/` 至少  
  - `product.swimlane-order-fulfillment.pgm`（三列 TB）  
  - `product.matrix-phase-role.pgm`（列×行）  
- [ ] hier_eval：纳入 partition 目录；暂作 smoke（可布局、无 panic）  
- [ ] 文档：dsl-spec「parse planned」→ 已落地；Hier 消费改为本文 PG-*  

**验收**：parse 已有测保持绿；新 showcase `cargo run -p plotgram-cli -- render …` 出 SVG；**§10 硬门槛**（无 partition 全量 fixture bit-identical）。

**非目标**：连续块、band 像素、矩阵 rank。

---

### PG-1 · 泳道最小闭环（仅 cross-axis columns，TB）≈ 1.5–2.5 周

**目标**：TB + **仅 columns** 时，同列节点层内成块、列声明序 = 几何左→右；跨列边仍走全局 rank。

- [ ] Compose：按 orientation 映射选出 **cross-axis** 轴（TB→columns）  
- [ ] 插入 Partition 连续块（每 `(column × rank)` Left/Right clamp 或等价）  
- [ ] `order_layers`：列块 precedence（声明序）；与 group clamp 共存策略写清  
- [ ] Metric：列 band Fit（含同列成员 ± pad）；空列 `PartitionBandMinSize`（标题/最小宽）  
- [ ] 未指派 cell：不进列块（自由区）  
- [ ] hier_eval：同列节点 cross 投影落在列 band 内；列序不颠倒  
- [ ] 代表图：三列订单泳道 — 同 rank 跨列对齐可读  

**刻意不做**：rows、LR orientation、Strong 特判、渲染底色。

**验收**：swimlane showcase 列序稳定、两次 run bit-identical；**§10 硬门槛**必过。

---

### PG-2 · Band 写权收口 + 可观测 ≈ 1 周

**目标**：列 band 成为 Metric 真源字段；Demand 空轴；可选外轨/调试暴露。

- [ ] `PartitionBandCoords` 进 `HierarchicalObs` 或 `LayoutResult`（择一，ADR 倾向可观测）  
- [ ] 空列保留 label 带宽（常数进 `group_frame` 旁或 `partition` 常量模块）  
- [ ]（可选）Channel outer rail 避让 column band（对齐 D₂ `group_obstacles` 先例）  
- [ ] debug-inspector / measure JSON 能看见 band  

**验收**：空列仍占位；obs 中 band 与节点落带一致；**§10 硬门槛**必过。

---

### PG-3 · 矩阵（main-axis rows）≈ 1.5–2 周

**目标**：`rows` + `columns`：行约束全局 rank 区间；列仍 cross 连续块。

- [ ] Ranking：有 row cell 的节点 rank ∈ 该行分配的层区间（全局一层表，非整图每行独立分层）  
- [ ] 行区间与 Network Simplex 的衔接：硬区间 vs soft（推荐：分层后投影/夹紧 + Demand，避免拆 NS）——**开 PG-3 前写一页短裁定**  
- [ ] Metric：row band T/B；与 LayerGap / shell 共存  
- [ ] hier_eval：`cell_row` 节点落在行 band；同列约束仍成立  
- [ ] matrix showcase  

**风险**：rank 区间与 FAS/跨行边张力大 → 不可行必须硬失败，禁止静默丢 cell。

**验收**：阶段×角色矩阵图可读；无 rows 的列-only 图相对 PG-1 bit-identical；**§10 硬门槛**必过。

---

### PG-4 · Orientation + 共址加固 ≈ 1–1.5 周

**目标**：四向 orientation 轴映射正确；与 Weak/Strong/group 同图不炸；产品门禁。

- [ ] LR/RL：rows 作 cross、columns 作 main（表 §5.1）单测四向 round-trip  
- [ ] Weak + partition、Strong + partition 各至少 1 fixture  
- [ ] group∩cell 同图：containment 双约束；成环 Infeasible + 单测  
- [ ] params：`partition_unassigned: free|reject`（默认 free）bind + hash  
- [ ]（可选）render 泳道标题/带背景——只读 band，不发明  
- [ ] roadmap / scope / shared/partition：**引擎消费 planned → 已落地**  
- [ ] architecture M5 验收句可勾  

**验收**：四向 smoke；同图 group+partition；文档状态同步；**§10 硬门槛**必过。

---

## 8. 模块落点（建议）

```text
crates/plotgram-layout/src/layout/hierarchical/
  compose/
    graph_index.rs     // PG-0: 拷贝 partition_cell
    partition_boundary.rs  // 新增：列/行连续块（或扩 boundary.rs）
    order.rs           // 消费 partition precedence
    rank.rs            // PG-3: 行区间（若需）
  metric/
    partition_bands.rs // 新增：band 变量 + Fit + Demand
  demand.rs            // PartitionBandMinSize
  params.rs            // partition_unassigned
  mod.rs               // validate；obs 暴露 bands
apps/showcase/hierarchical/partition/*.pgm
docs/design/layout/shared/partition.md   // 状态翻为已消费
```

---

## 9. 工作量总览

| 切片 | 粗工期 | 依赖 |
|------|--------|------|
| **PG-0** | 0.5–1 周 | 无（可立即开） |
| **PG-1** | 1.5–2.5 周 | PG-0 |
| **PG-2** | ~1 周 | PG-1 |
| **PG-3** | 1.5–2 周 | PG-1（可与 PG-2 部分并行） |
| **PG-4** | 1–1.5 周 | PG-1 + PG-3 |
| **合计** | ≈ 5–8 周 | 串行主路径 |

**MVP 可交付点**：PG-0+PG-1（TB 泳道）即可给产品用；矩阵与四向为加固。

---

## 10. 硬门槛：无 partition 图 bit-identical

Partition 实施是**增量路径**：默认图不得因接线 / 连续块 / band / 参数默认值而改几何。下列为**每一切片合入前的硬门槛**（失败 = 不合并）。

### 10.1 适用范围

| 集合 | 要求 |
|------|------|
| **无 partition 图** | `Graph.partition == None`，且全体节点无 `partition_cell`（当前几乎全部 hier showcase / hier_eval fixture） |
| **有 partition 图** | 不要求相对旧 baseline 不变；另建 partition 目录 baseline / 门禁 |

### 10.2 比对什么

相对**该切片开工前**的重建 baseline（同一 debug/`cargo run` 路径，禁止拿陈旧 release binary）：

1. **节点中心 / 框矩形**（Weak 组框、Strong MacroBlock 框）  
2. **边折点序列**（正交路径）  
3. **既有 hier_eval 硬门禁**（无重叠、端口落界、组包含、穿组等）仍全绿  

允许变的：仅观测字段增列且值为空/缺省（例如尚未写 `partition_band_coords`）；**禁止**借机改默认 params 导致无 grid 图位移。

### 10.3 如何守

- 每 PG 合入前：对无 partition fixture 跑既有 baseline / hier_eval 回归；有几何 diff → **阻断**，先查是否误触 Ordering/Metric 公共路径。  
- 代码纪律：`partition_grid.is_none()`（且无 cell）时，**不**插入 PartitionBoundary、**不**发布 Partition Demand、**不**改 order precedence / band 变量集合。  
- PG-2/PG-3/PG-4 若改公共 Metric/Channel：同样先过本门槛，再验有 grid 图。

### 10.4 切片对照

| 切片 | 本门槛 |
|------|--------|
| PG-0 | 必过（接线零副作用） |
| PG-1 | 必过（连续块/band 仅在有 columns 时启用） |
| PG-2 | 必过（obs/外轨不得波及无 grid） |
| PG-3 | 必过；另：「无 rows 的列-only 图」相对 PG-1 bit-identical |
| PG-4 | 必过；orientation/params 默认不得改无 grid 几何 |

---

## 11. 风险与裁定

| 风险 | 裁定 |
|------|------|
| 与 GroupBoundary 双套 clamp 冲突 | 统一 precedence 构造；硬环 → Infeasible |
| 行区间破坏 NS 最优 | PG-3 先「分层后夹紧 + Demand」，不开每行独立分层 |
| Strong 局部 cell 与全局 band | expand 后只写一次全局 band |
| 未指派节点乱跑 | 默认 free；`reject` 作显式 param |
| 渲染抢写权 | 只读 band；无 band 不画泳道底 |
| 公共路径误伤无 grid 图 | **§10 硬门槛**；无 grid 早退，禁止「顺手」改默认 |

---

## 12. Definition of Done（整段 PartitionGrid / M5）

1. DSL/model/parse 事实进入 Hier（不再丢 cell）。  
2. TB 列泳道：连续块 + band；跨列全局 rank 对齐。  
3. 矩阵：行 band + 列块同时成立。  
4. 四向 orientation 轴映射正确。  
5. 与 group / Weak / Strong 同图可证（或硬失败可诊断）。  
6. 无 group 冒充、无每泳道独立分层、无 Ink 发明 cell。  
7. showcase + hier_eval 门禁；文档 planned → 已落地。  
8. **§10**：无 partition 全量 fixture 相对开工前 baseline **bit-identical**（硬门槛，贯穿 PG-0–PG-4）。

---

## 13. 修订记录

| 日期 | 说明 |
|------|------|
| 2026-08-11 | 初版：产品选型泳道/矩阵；PG-0–PG-4；钉死与 group/Channel/Strong 边界；parse/model 已备、Hier 丢 cell 为 PG-0 入口 |
| 2026-08-11 | 增 §10：无 partition 图 bit-identical 硬门槛（适用范围 / 比对物 / 守法 / 切片对照）；写入 DoD |
