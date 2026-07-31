# ADR-006: Engine 入口、边写者与 crate 拆分

> 状态：accepted  
> 日期：2026-07-30（修订：算法实现收拢为 engine 内模块；2026-07-31 增加 `plotgram-algo` 零件 crate）  
> 关联：ADR-001、ADR-005、[`model-boundary.md`](../model-boundary.md)

## 背景

布局与路由体量大，Trait 若留在门面 `plotgram-engine`，将来抽出 Hier 独立 crate 会环依赖。内建正交与独立 EdgeRouter 需共用无策略原语。

早期曾为 stub 单独建 `layout-hierarchical` / `route-core` / `route-orthogonal` 三个 crate，粒度过碎。现收拢为 **engine 内模块**，模块边界按「日后原样搬出」来画。

## 决策

### 1. 入口

- Engine 只收 [`LayoutContract`](../../crates/plotgram-model/src/contract.rs)：`layout`、`edge_routing`、`graph`、**`node_sizes`**。  
- **不**收 `profile` / theme。缺 `node_sizes` → 错误。  
- 出口：[`LayoutResult`](../../crates/plotgram-model/src/result.rs)。

### 2. 边几何写者（`edge_routing`）

| `edge_routing` | 边路径写者 | 端口决议 |
|----------------|------------|----------|
| `None` | **Layout 内建 Ink** | Layout 组合相 |
| `Some(name)` | **`EdgeRouter`** | Layout 组合相（Router 不发明侧） |

### 3. Crate / 模块（现行）

**独立 crate（保留）：**

| Crate | 职责 |
|-------|------|
| `plotgram-model` | 数据 |
| `plotgram-engine-api` | `LayoutAlgorithm` / `EdgeRouter` / `LayoutError`（**禁止**放进 model 或门面） |
| `plotgram-algo` | **共享算法零件**（VPSC / FAS / 交叉计数 / orientation / track / 正交规范化等）；无管线、无 Contract；见 [`PARTS.md`](../../crates/plotgram-algo/PARTS.md) |
| `plotgram-engine` | `run` + 注册表 + **in-tree** `layout::*` / `route::*`（消费 algo） |
| `plotgram-pipeline` | 编排 |
| `plotgram-parse` / `content` / `render` / `cli` | 各司其职 |

**engine 内模块（可后拆）：**

```text
plotgram-engine
  layout/hierarchical/     → 将来 plotgram-layout-hierarchical
  route/core/              → 将来 plotgram-route-core
  route/orthogonal.rs      → 将来 plotgram-route-orthogonal
  run / registry / finalize
```

依赖纪律：

```text
model ← engine-api
         algo（零件；当前可不依赖 model）
              ↑
         layout/route 实现 ← engine 门面（只组装，实现不依赖 run）
```

- **禁止**实现模块依赖 `run` / 注册表的「门面逻辑」形成环。  
- **禁止** `plotgram-algo` 依赖 `plotgram-engine`。  
- 内建 Ink 属于 hierarchical，可调用 `algo` 与 `route::core`；不是 `EdgeRouter`。

### 4. 何时再拆 layout/route crate

当某个 `layout/*` 或 `route/*` **体量与编译时间**明显拖累 engine、或需独立发布/feature 裁剪时，再抽成 workspace 成员；Trait 已在 `engine-api`，搬迁成本可控。

**例外（已提前拆）**：`plotgram-algo` 在零件阶段即独立 —— 理由是编译隔离（v1 教训）与 M0 地基可并行验收；**不**再拆成多个微 crate（禁止 `plotgram-vpsc` 等）。

### 5. Group 尺寸

叶子 size 在 layout 前；group 框在 `run` 末尾包络写出。

## 含义

- **零件**：优先落在 `plotgram-algo`（见 PARTS.md），带表驱动单测；再由 layout/route 接线。  
- **布局/路由算法**：先加 `engine` 内模块 + 注册；长大再 extract。  
- 编排：`pipeline`（parse → measure → `engine::run` → render）；CLI 保持薄。

## 备选方案（未采用）

| 方案 | 原因 |
|------|------|
| Trait 放 model | 污染数据层 |
| Trait 留门面、无 api crate | 抽出 Hier 时环依赖 |
| stub 期就为每个算法建 crate | 过碎；已收回 |
