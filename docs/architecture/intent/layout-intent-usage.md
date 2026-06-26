# Layout Intent 使用指南

Layout Intent(布局意图)是 Drawify 的布局叠加层,允许在不修改 diagram 源码 `relations` 的前提下,向布局算法注入额外的拓扑/几何约束,并返回每条意图的满足度报告。

> 设计依据:[layout-intent-optimized.md](./layout-intent-optimized.md) v2.1

---

## 核心概念

### 为什么需要 Intent?

传统布局完全由 `diagram.relations`(真实边)驱动。但有时用户希望施加额外的布局约束(如"A 应在 B 下方"、"A、B、C 垂直对齐"),而又不想:

- 在 diagram 里画幽灵边(会破坏 `relations[i] ↔ edges[i]` 索引契约)
- 修改源码(意图是渲染期参数,不是图本身的一部分)

Layout Intent 通过 **overlay 叠加层** 解决:意图作为独立参数传入,布局算法消费后产出 `RefinementReport` 报告每条意图的满足状态。

### 两类意图

| 类型 | 枚举 | 消费阶段 | 作用 |
|------|------|----------|------|
| **拓扑意图** `TopologyIntent` | `Below` / `Above` | `strategy.compute_with_overlay` 内部 | 影响 Sugiyama 分层 rank 排序 |
| **几何意图** `GeometricIntent` | `Pin` / `AlignVertical` / `AlignHorizontal` | grid snap 前的 `apply_geometric_refinement` | 锁定节点坐标 / 对齐节点中心 |

### 满足状态 `IntentStatus`

| 状态 | 含义 |
|------|------|
| `Satisfied` | 意图被完全满足 |
| `Partial` | 意图部分满足(如跨组对齐仅对齐同组节点、穿障修正破坏了对齐) |
| `Conflicted` | 意图冲突(如引入环、矛盾意图),被跳过 |
| `NotFound` | 意图引用的节点不存在 |

---

## 数据结构

### `LayoutIntentOverlay`

意图叠加层,包含 `topology` 和 `geometric` 两个向量。

```rust
pub struct LayoutIntentOverlay {
    pub topology: Vec<TopologyIntent>,
    pub geometric: Vec<GeometricIntent>,
}
```

JSON 表示:

```json
{
  "topology": [
    { "kind": "below", "from": "a", "to": "b" },
    { "kind": "above", "from": "c", "to": "d" }
  ],
  "geometric": [
    { "kind": "pin", "node": "a", "axis": "both" },
    { "kind": "align_vertical", "nodes": ["a", "b", "c"] },
    { "kind": "align_horizontal", "nodes": ["d", "e"] }
  ]
}
```

### 拓扑意图 `TopologyIntent`

```rust
pub enum TopologyIntent {
    Below { from: String, to: String },  // from 应在 to 下方
    Above { from: String, to: String },  // from 应在 to 上方
}
```

**语义说明(Sugiyama rank 方向):**

- `Below(A, B)` → 注入边 `B → A` → `rank(A) > rank(B)` → A 在 B 下方 ✅
- `Above(A, B)` → 注入边 `A → B` → `rank(A) < rank(B)` → A 在 B 上方 ✅

> 在 Sugiyama 布局中,边 `X → Y` 意味着 `rank(X) < rank(Y)`(X 在 Y 上方)。

### 几何意图 `GeometricIntent`

```rust
pub enum GeometricIntent {
    Pin { node: String, axis: PinAxis },
    AlignVertical { nodes: Vec<String> },    // x 中心对齐
    AlignHorizontal { nodes: Vec<String> },  // y 中心对齐
}

pub enum PinAxis {
    Both,  // 锁定 x、y 两轴
    X,     // 仅锁定 x 轴
    Y,     // 仅锁定 y 轴
}
```

- `Pin`:锁定节点当前坐标,标记为 pinned 跳过后续 grid snap(仅轴约束,不含绝对坐标)
- `AlignVertical`:将 nodes 的 x 中心对齐到均值,单轮重叠消除(沿 y 轴推开),对齐节点加入 pinned
- `AlignHorizontal`:将 nodes 的 y 中心对齐到均值,单轮重叠消除(沿 x 轴推开)

### `RefinementReport`

```rust
pub struct RefinementReport {
    pub results: Vec<IntentResult>,
    pub satisfied: usize,
    pub partial: usize,
    pub conflicted: usize,
    pub not_found: usize,
}

pub struct IntentResult {
    pub index: usize,        // 在 overlay.topology/geometric 中的索引
    pub kind: &'static str,  // "below" / "above" / "pin" / "align_vertical" / "align_horizontal"
    pub status: IntentStatus,
    pub message: Option<String>,
}
```

---

## 冲突处理

| 冲突类型 | 检测时机 | 处理 |
|----------|----------|------|
| 意图节点不存在 | overlay 解析时 | `NotFound`,跳过 |
| 拓扑意图引入环 | `build_graph_with_overlay` 后 DFS | `Conflicted`,跳过该意图边 |
| `Below(A,B)` + `Above(A,B)` 矛盾 | overlay 解析时去重 | 保留先声明者,后者 `Conflicted` |
| 跨组拓扑意图(Architecture V2) | strategy 内部判断 | `Partial`,仅组内生效 |
| 对齐节点跨组 | `apply_geometric_refinement` | `Partial`,仅对齐同组节点 |
| 对齐后重叠无法消除 | 重叠消除单轮失败 | `Partial` |
| 穿障修正破坏对齐 | `refine::run_refine` 后比对 | `Partial`(首期仅观测,不回滚) |

---

## 使用方式

### 1. Rust 核心 API

#### `compute_layout_with_plan_and_overlay`

布局 dispatch 入口,接受 overlay 参数并返回 `RefinementReport`。

```rust
use drawify_core::layout::{compute_layout_with_plan_and_overlay, LayoutIntentOverlay, TopologyIntent};

let overlay = LayoutIntentOverlay {
    topology: vec![TopologyIntent::Below {
        from: "a".into(),
        to: "b".into(),
    }],
    geometric: vec![],
};

let (layout, report) = compute_layout_with_plan_and_overlay(
    diagram,
    layout_plan,
    Some(&overlay),
)?;

if let Some(report) = report {
    println!("satisfied: {}, conflicted: {}", report.satisfied, report.conflicted);
    for r in &report.results {
        println!("  [{}] {:?}: {}", r.index, r.status, r.message.as_deref().unwrap_or(""));
    }
}
```

#### `render_output_with_report`

渲染流水线入口,返回 `RenderOutputWithReport { output, report }`。

```rust
use drawify_core::pipeline::render_output_with_report;
use drawify_core::render::RenderRequest;

let mut request = RenderRequest::new(&prepared, RenderFormat::Svg);
request.layout_overlay = Some(&overlay);

let result = render_output_with_report(&request)?;
let svg_text = match result.output {
    RenderOutput::Text(t) => t,
    _ => unreachable!(),
};
if let Some(report) = result.report {
    // 处理报告
}
```

### 2. WASM API

#### `WasmRenderOptions.layout_intents`

```typescript
// JavaScript
const options = {
  theme_id: "builtin.clean-light",
  layout_intents: {
    topology: [
      { kind: "below", from: "a", to: "b" },
      { kind: "above", from: "c", to: "d" }
    ],
    geometric: [
      { kind: "pin", node: "a", axis: "both" },
      { kind: "align_vertical", nodes: ["a", "b", "c"] }
    ]
  }
};

const json = wasm.render_with_options(source, "svg", JSON.stringify(options));
const result = JSON.parse(json);
console.log(result.success);           // true
console.log(result.text);             // SVG 字符串
console.log(result.refinement_report); // { satisfied: 3, partial: 0, ... }
```

#### `RenderResult.refinement_report`

```typescript
interface RenderResult {
  success: boolean;
  format: string;
  text: string | null;
  errors: string[];
  warnings: string[];
  refinement_report?: RefinementReport;  // 仅当 layout_intents 存在时
}

interface RefinementReport {
  results: IntentResult[];
  satisfied: number;
  partial: number;
  conflicted: number;
  not_found: number;
}
```

### 3. Server HTTP API

#### `POST /render` 请求体

```bash
curl -X POST http://localhost:6080/render \
  -H "Content-Type: application/json" \
  -d '{
    "source": "diagram flowchart { entity a \"A\"; entity b \"B\"; a -> b }",
    "format": "svg",
    "layout_intents": {
      "topology": [
        { "kind": "below", "from": "a", "to": "b" }
      ],
      "geometric": []
    }
  }'
```

#### `X-Drawify-Refinement-Report` 响应头

成功响应会携带 `X-Drawify-Refinement-Report` 头(仅当请求体包含 `layout_intents` 时),值为 JSON 序列化的 `RefinementReport`:

```
HTTP/1.1 200 OK
Content-Type: image/svg+xml
X-Drawify-Format: svg
X-Drawify-Valid: true
X-Drawify-Refinement-Report: {"results":[{"index":0,"kind":"below","status":"Conflicted","message":"..."}],"satisfied":0,"partial":0,"conflicted":1,"not_found":0}

<svg>...</svg>
```

解析示例(Node.js):

```javascript
const report = JSON.parse(response.headers['x-drawify-refinement-report']);
console.log(`Satisfied: ${report.satisfied}, Conflicted: ${report.conflicted}`);
```

---

## 示例场景

### 场景 1:强制节点上下顺序

需求:A 必须在 B 下方,但 diagram 中没有 A→B 的边。

```json
{
  "topology": [{ "kind": "below", "from": "a", "to": "b" }],
  "geometric": []
}
```

效果:注入边 `B → A`,Sugiyama 分层后 `rank(A) > rank(B)`,A 显示在 B 下方。

### 场景 2:节点垂直对齐

需求:A、B、C 三个节点的 x 中心对齐。

```json
{
  "topology": [],
  "geometric": [{ "kind": "align_vertical", "nodes": ["a", "b", "c"] }]
}
```

效果:三个节点的 x 中心对齐到均值,沿 y 轴单轮推开重叠,对齐节点跳过后续 grid snap。

### 场景 3:锁定节点位置

需求:节点 A 的位置在 grid snap 时不被移动。

```json
{
  "topology": [],
  "geometric": [{ "kind": "pin", "node": "a", "axis": "both" }]
}
```

效果:节点 A 在 grid snap 阶段被跳过(x、y 都不吸附)。

### 场景 4:混合意图

```json
{
  "topology": [
    { "kind": "below", "from": "a", "to": "b" },
    { "kind": "above", "from": "c", "to": "d" }
  ],
  "geometric": [
    { "kind": "pin", "node": "a", "axis": "x" },
    { "kind": "align_vertical", "nodes": ["b", "c", "d"] }
  ]
}
```

报告示例:

```json
{
  "results": [
    { "index": 0, "kind": "below", "status": "Satisfied", "message": null },
    { "index": 1, "kind": "above", "status": "Satisfied", "message": null },
    { "index": 0, "kind": "pin", "status": "Satisfied", "message": null },
    { "index": 1, "kind": "align_vertical", "status": "Satisfied", "message": null }
  ],
  "satisfied": 4,
  "partial": 0,
  "conflicted": 0,
  "not_found": 0
}
```

### 场景 5:冲突检测

真实边 `A → B`,意图 `Below(A, B)` → 注入边 `B → A` → 环 `A → B → A`。

```json
{
  "topology": [{ "kind": "below", "from": "a", "to": "b" }],
  "geometric": []
}
```

报告:

```json
{
  "results": [
    {
      "index": 0,
      "kind": "below",
      "status": "Conflicted",
      "message": "intent edge b→a forms a cycle with existing edges"
    }
  ],
  "satisfied": 0,
  "partial": 0,
  "conflicted": 1,
  "not_found": 0
}
```

意图被跳过,布局按真实边 `A → B` 进行(`rank(A) < rank(B)`,A 在 B 上方)。

---

## 支持的布局算法

| 算法 | 拓扑意图 | 几何意图 | 备注 |
|------|----------|----------|------|
| `sugiyama-v2` | ✅ 原生 | ✅ | 通过 `EdgeMeta` 保护意图边不被 FAS 反转 |
| `flowchart` | ✅ 原生 | ✅ | 同上(FLOWCHART_PRESET) |
| `er` | ✅ 原生 | ✅ | 同上(ER_PRESET) |
| `architecture` | ✅ 仅组内 | ✅ | 跨组拓扑意图标记 `Partial` 并跳过 |
| 其他 | ❌ 忽略 | ✅ | 非 Sugiyama 系布局无 rank 概念,拓扑意图标记 `Partial` |

---

## 实现细节

### FAS 保护(避免意图边被反转)

Sugiyama 的 `greedy_cycle_reversal` 会反转边以破环。意图边被标记为 `reversible: false`,不参与 FAS 反转:

```rust
// crates/drawify-core/src/layout/node/sugiyama_v2/graph.rs
pub(super) struct EdgeMeta {
    pub kind: EdgeKind,        // Real | Intent
    pub reversible: bool,      // 意图边 = false
}
```

Architecture V2 采用不同策略:在 FAS **之后**注入意图边,因此意图边永不被反转。

### 穿障修正后对齐完整性检查

`refine::run_refine` 可能推开节点破坏对齐。设计 §5.3.1 规定首期"仅观测,不回滚":

```rust
// 在 refine::run_refine 之后调用
intent::geometric::check_alignment_after_refine(&result, &pinned, ov, &mut report);
```

若对齐被破坏(节点中心偏移 > 1.0px),对应的 `align_*` 意图从 `Satisfied` 降级为 `Partial`。

### `PinSet` 结构

记录被 `Pin` / `Align*` 保护的节点,供 `grid_snap::snap_layout_to_grid` 跳过:

```rust
pub struct PinSet {
    pub full: HashSet<String>,              // PinAxis::Both
    pub x_only: HashSet<String>,            // PinAxis::X
    pub y_only: HashSet<String>,            // PinAxis::Y
    pub aligned_vertical: HashSet<String>,   // AlignVertical
    pub aligned_horizontal: HashSet<String>, // AlignHorizontal
}
```

---

## 性能影响

| 操作 | 复杂度 | 备注 |
|------|--------|------|
| 意图校验 | O(I + V + E) | I = 意图数,环检测 DFS |
| `build_graph_with_overlay` | O(V + E + I) | 额外 I 条意图边 |
| `greedy_cycle_reversal` 改造 | O(V + E + I) | 额外判断 reversible 标记 |
| `apply_pin` | O(1) per node | |
| `apply_align_*` | O(K log K) | K = 对齐节点数 |
| 重叠消除 | O(K²) 最坏 | K 通常 < 10 |
| `snap_layout_to_grid` pinned 跳过 | O(1) 查找 | HashSet |

**结论:** 性能影响可忽略(< 1ms),远小于布局算法本身。

---

## 测试覆盖

实现包含以下测试(共 49 个 intent 相关测试 + 7 个端到端测试):

- **拓扑意图校验** (topology.rs):节点存在性、矛盾去重、环检测、自环、重复意图
- **拓扑意图集成** (integration_tests):below/above rank 排序、环检测拒绝、FAS 保护、多意图混合状态
- **几何意图** (geometric.rs):pin 标记、align 对齐、重叠消除、跨组对齐、单节点/不存在节点
- **穿障修正后对齐检查**:对齐保持 Satisfied、对齐破坏降级 Partial、不覆盖更严重状态
- **端到端** (pipeline/render.rs):SVG/ASCII 路径、Satisfied/Conflicted/NotFound/空报告

运行测试:

```bash
cargo test -p drawify-core --lib layout::intent
cargo test -p drawify-core --lib pipeline::render
```

---

## 相关文档

- [设计文档:layout-intent-optimized.md](./layout-intent-optimized.md) — 完整设计与实现路线
- [WASM 模块设计](./wasm-module.md) — WASM API 总览
- [Server API 使用说明](./drawify-server-api.md) — HTTP 接口文档
