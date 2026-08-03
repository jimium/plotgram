# 布局调试检视器（Layout Debug Inspector）

> 状态：**现行设计档（目标契约）**  
> 日期：2026-08-03  
> 范围：开发期检视工具（**不是** playground / 不是产品渲染）  
> 约束入口：[写权纪律](write-authority.md) · [ADR-001](../adr/001-diagram-type-not-in-engine.md) · [ADR-009](../adr/009-layout-result-decorations.md) · [ADR-007](../adr/007-layout-facts-llm-channel.md)  
> 代码落点（目标）：各 layout 核投影 Trace · CLI · 最小检视页（跨核壳 T4 上提）· 可选 `showcase/debug/`

本文钉死：**布局调试 Trace 旁路与检视壳**。首期只交付 **hierarchical 的具体 trace + CLI**；跨核统一信封与 UI 插件壳是**后置抽象**（第二个核落地后再提炼，见 §9）。各算法只实现自己的 **extension profile**，把内部决策旁路导出，而不污染产品 `render`。

Hier 是**第一个** profile 落地对象，不是唯一消费者。算法专属字段见：

| 内核 | 扩展剖面文档 |
|------|----------------|
| hierarchical | [hierarchical/debug-profile.md](hierarchical/debug-profile.md) |
| tree / sequence / circular | 内核毕业时按本文 §5 增补同级短文 |

实现进度不进本文；当前能力以代码与里程碑验收为准。

---

## 0. 一句话目标

```text
任意 layout.name
  → 跑完整 layout（与产品同路径）
  → 旁路导出 LayoutDebugTrace { common…, extension: <algo> }
  → 同一套 Inspector UI：按 layout 装载叠层插件
```

首期只做到前两步（hierarchical）；「同一套 Inspector UI」是目标形态，首期壳从简。

**不做**：按图种 / profile 名分支；产品 SVG 长满 debug；检视器回写坐标；为未实现能力画假几何。

---

## 1. 动机

布局质量问题多数来自**上游离散决策**，不是最终像素「差一点」。今日对外往往只有：

```text
LayoutOutput { nodes, edges }   // 最终框 + 折线
```

内部 Plan / 轴序 / dummy / 生命线决议等算完即丢。需要：

1. **统一旁路信封** —— CLI、snapshot、UI、将来 ADR-007 Facts 共用  
2. **算法可插拔扩展** —— hier 的 rank 与 sequence 的 lifeline slot 不应挤在同一扁平字段袋里  
3. **产品 render 零污染** —— 叠层是调试消费者，不是 Decoration（ADR-009）

---

## 2. 与邻近设计的边界

| 设计 | 关系 | 本检视器 |
|------|------|----------|
| **产品 render** | 作者可见几何 + theme | **不**往默认 SVG 塞 debug |
| **ADR-009 Decoration** | 布局派生的**产品**几何 | Debug 叠层 **不是** Decoration |
| **ADR-007 Layout Facts** | 引擎→LLM 定性谓词 | 可共享 Trace；Facts 后置消费者 |
| **Showcase** | 样例监视 | 可链到检视器；不默认批产 Trace |
| **Playground** | 编辑 DSL | 非目标 |
| **各内核 architecture** | Plan / 相契约真源 | Trace 是其**只读投影**，不另造决策 |

判断句：若修法是「在 `render_svg` 里 `if layout == hierarchical` 画 rank」，先问「为何不走 Trace extension + DebugPainter 插件」。

---

## 3. 硬约束

| # | 约束 | 含义 |
|---|------|------|
| D1 | **旁路导出** | 经 `LayoutDebugTrace`；默认产品路径零开销或可剥离 |
| D2 | **单写者不变** | Trace = 只读投影；UI / DebugPainter 不得回写几何 |
| D3 | **产品 render 零新决策** | 正式管道不读 Trace |
| D4 | **确定性** | 稳定序；可 snapshot |
| D5 | **按实现规范名扩展，不按图种** | `extension.kind == 实现规范名`；禁止 `profile` / `DiagramType` 分支（ADR-001） |
| D6 | **缺失即显式** | 某段未实现 → `null` + `reason` / `notes[]`，禁止示意假数据 |
| D7 | **信封稳定、剖面可增** | `common` 破坏性变更升 `schema_version`；新算法只加 extension 变体 |

---

## 4. 架构总览

```text
DSL → parse → measure → LayoutContract
         → LayoutAlgorithm::layout
              ├─► LayoutOutput          （产品几何，各核相同出口形状）
              └─► LayoutDebugTrace      （旁路；common + extension）
                       │
         ┌─────────────┼─────────────┐
         ▼             ▼             ▼
   产品 render    DebugPainter    Inspector UI
                  （按 extension     （共用壳 +
                   kind 选插件）      叠层插件表）
```

### 4.1 Trace 产生机制（钉死）

各核实现是无状态的（`layout()` 的局部变量算完即丢，不存在「本次内部状态」可供投影），故钉死**重跑收集**路线：

```text
trait LayoutDebugTraceProvider {
  // 规范名（实现在 Registry 的主注册名；别名规则见 §5.1）
  fn canonical_name(&self) -> &'static str;
  // 拿同一份 LayoutInput，重跑与产品相同的管线，沿途收集中间决策
  fn build_debug_trace(&self, input: LayoutInput) -> LayoutDebugTrace;
}
```

- 重跑只为收集：trace 的几何与产品几何出自同一份算法决策，不是第二真源；确定性要求——同输入两次运行逐字节一致（以 snapshot 验收）。
- 产品路径（`LayoutAlgorithm::layout`）零侵入：不为 debug 存状态，不读 trace。
- 未实现 provider 的核：CLI **硬错误**（`trace unsupported for layout X`），不产出空壳 Trace；产品 render 不受影响。
- Hier 首期实现；其它核毕业时实现。

### 4.2 职责

| 组件 | 写 | 读 | 禁止 |
|------|----|----|------|
| 各 Layout 核 | 本核 Plan/Metric（照旧） | — | 为 debug 改决策 |
| Trace 投影器（每核一份） | `extension` 段 | 本核管线（重跑收集） | 写几何；改决策；HashMap 序 |
| 共用序列化 | JSON 信封 | Trace | 解释算法语义 |
| 产品 render | SVG 笔触 | LayoutResult | 读 Trace |
| DebugPainter 插件 | 叠层几何 | common + 对应 extension | 成为第二真源 |
| Inspector 壳 | 仅 UI 状态 | Trace + SVG | 写回引擎 |

### 4.3 Crate 落点

| 能力 | 落点 | 说明 |
|------|------|------|
| 信封类型 `LayoutDebugTrace` | 目标：`plotgram-engine-api` 或 `plotgram-model`（稳定后）；首期可 layout 内 + JSON | 跨核共享 |
| 各核投影 | `plotgram-layout/src/layout/{name}/debug.rs` | 靠近内部 IR |
| CLI | `plotgram-cli` | 与 layout.name 无关的入口 |
| DebugPainter 注册表 | 独立模组：`kind → paint_fn` | 禁止打进默认 `render_svg` |
| Inspector | 静态页一处 | 读 `extension.kind` 装载面板 |

---

## 5. `LayoutDebugTrace` 信封

### 5.1 顶层

```text
LayoutDebugTrace {
  schema_version: u32
  layout: string                 # Registry 注册名如实记录（`architecture` 等别名也如实；见规则）
  orientation: string?           # 若该核有方向概念；否则 null
  space: "physical"              # 首期钉死 physical；见 §5.4

  # —— 跨核 common（有则填，无则空数组 / null）——
  common: CommonDebug

  # —— 算法扩展（带标签联合；一次 layout 恰一种）——
  extension: ExtensionDebug

  notes: string[]                # 人类可读限制，如 "hierarchical: no channel in this build"
}

CommonDebug {
  nodes: NodeCommonDebug[]       # 产品 NodeId + frame（便于点选，不替代 LayoutOutput）
  edges: EdgeCommonDebug[]       # id + 端点 + 可选最终 path
  groups: GroupCommonDebug[]     # 有组则填
  decorations: DecorationRef[]? # 仅引用 ADR-009 id/type，不复制整份产品几何
}

ExtensionDebug =
  { kind: "hierarchical", … HierarchicalExtension }
  | { kind: "tree", … TreeExtension }
  | { kind: "sequence", … SequenceExtension }
  | { kind: "circular", … CircularExtension }
  # 新核：加变体；旧 UI 对未知 kind 显示「无叠层插件」+ raw JSON
```

**规则：**

1. `extension.kind` 必须等于实现的**规范名**（Registry 主注册名）；`layout` 如实记录注册名。别名存在时（`architecture` → hierarchical 实现）`layout != kind` 合法；UI / 插件表按 `kind` 装载。  
2. Common **不**承载 rank/lifeline 等算法私有概念。  
3. 未知 `kind`：检视器仍可浏览 common + 原始 extension JSON；不得猜测叠层。  
4. 字段原则：能算则填、不能则无——不设「占位」伪值（D6）；不为未来能力预立空字段（如参数 hash，待 Diagnostics 落地时一并加）。

### 5.2 Common 字段（最小）

```text
NodeCommonDebug {
  id: NodeId
  frame: Rect?
  center: Point?
}

EdgeCommonDebug {
  edge_id: EdgeId
  source: NodeId
  target: NodeId
  path: Point[]?    # DeferToRouter 时为 null 属合法状态，非缺失；自环为其桩几何
}

GroupCommonDebug {
  group_id: GroupId
  parent: GroupId?
  frame: Rect?
  frame_source: "metric" | "finalize-bbox" | "none"
}
```

### 5.3 扩展剖面（各核自洽）

扩展的**权威字段表**写在各核短文；本文只规定形状与示例。

#### hierarchical（详见 [debug-profile](hierarchical/debug-profile.md)）

```text
HierarchicalExtension {
  elems: ElemDebug[]             # real + virtual
  layers: LayerDebug[]
  edge_plans: HierEdgeDebug[]    # reversed / segments / dummy_chain / 自环占位
  ports: PortDebug[]
  channels: ChannelDebug?        # absent | present
  metrics: MetricDebug?
}
```

#### tree（示意，待 tree 架构钉死后填真源）

```text
TreeExtension {
  roots: NodeId[]
  parent: { child: NodeId, parent: NodeId }[]
  depth: { id: NodeId, depth: u32 }[]
  contour_or_levels: …?         # 按实际算法 IR
}
```

#### sequence（示意）

```text
SequenceExtension {
  participants_order: NodeId[]
  message_slots: { edge_id, time_index, … }[]
  lifeline_refs: DecorationId[]  # 与 ADR-009 对齐，不重复画产品几何于 Trace
  activations: …
}
```

#### circular（示意）

```text
CircularExtension {
  components: { id, member_nodes: NodeId[] }[]
  angles: { id: NodeId, angle_rad: f64 }[]
  radii: …
}
```

示意字段**不是**实现契约；以免未设计的核被文档锁死。锁死以各核 `debug-profile.md` / architecture 为准。

### 5.4 坐标空间

首期钉死 **physical**（与产品 SVG 对齐）：所有几何字段由投影器经该核自己的 orientation 模块（hier 为 `orient.rs`）转成 physical 后导出；**trace 中不出现 canonical 值**——以防调试层的方向换算遗漏复刻算法层的同类 bug。未来若某核确需导出 canonical，届时再以顶层 `space` 声明钉死解释；UI 叠层以 Trace 声明为准。

### 5.5 与 Diagnostics

```text
LayoutDiagnostics   → 警告 / 放宽 / 不可行（跨核目标）
LayoutDebugTrace    → 决策结构可视化
```

注意：`LayoutDiagnostics` 目前**尚无输出通道**（`LayoutOutput` 无此字段，见 hier mvp-scope §0.1），是未来项；Trace 不依赖它、不等它。两者落地后并列产出，不合并成单一根对象；UI 可分栏。

---

## 6. Inspector UI（共用壳）

### 6.1 信息架构

```text
┌──────────────┬────────────────────────────┬─────────────────┐
│ 源 / 列表     │ 画布                        │ 检视            │
│ 打开产物      │ 产品几何 + 叠层插件（按 kind） │ common 字段      │
│ showcase 链   │ 平移缩放 / 点选              │ extension 字段   │
└──────────────┴────────────────────────────┴─────────────────┘
```

### 6.2 叠层插件表

| 层来源 | 层 ID 例 | 谁提供 paint | 何时出现 |
|--------|----------|--------------|----------|
| **common** | `product`, `groups` | 共用 | 始终可注册 |
| **extension** | hier: `ranks`,`dummies`,`reversed`,`ports`,`channels` | hier 插件 | `kind==hierarchical` |
| **extension** | seq: `time-grid`,`message-index` | sequence 插件 | `kind==sequence` |
| **extension** | tree: `depth-bands`,`subtree-frames` | tree 插件 | `kind==tree` |

壳逻辑：

```text
layers = common_layers ∪ plugins[trace.extension.kind]
unknown kind → common_layers only + raw JSON panel
```

### 6.3 交互（跨核）

点选优先解析 **common** id；若命中 extension 专有对象（如 dummy key），由当前插件解释。  
URL hash 保存：`layout`（冗余校验）、层开关、选中 id。

### 6.4 实现形态

| 产物 | 说明 |
|------|------|
| `*.svg` | 产品，不变 |
| `*.trace.json` | 本信封 |
| `*.overlay.svg`（可选） | 当前 kind 的默认叠层合集；UI 内仍应用开关更佳 |

首期：静态页 + JSON；Rust DebugPainter 可后补。

---

## 7. CLI

```bash
plotgram debug-layout <file.pgm> -o out.trace.json
plotgram render <file.pgm> -o out.svg --emit-trace out.trace.json
```

- 入口**不**按图种分支；调用当前 contract 的 layout 核的 provider。  
- 核无 provider → **硬错误**并明确提示（`trace unsupported for layout X`）；不产出空壳 Trace。

---

## 8. 模块影响（跨核视角）

| 模块 | 影响 |
|------|------|
| **信封 + CLI + UI 壳** | 一次性中等；之后新核只加插件 |
| **每个 layout 核** | 实现投影器（该核中等） |
| **产品 render** | **接近零** |
| **model** | 信封稳定后上提；首期可延迟 |

新核接入清单：

1. 写 `layout/{name}/debug-profile.md`（字段真源）  
2. 实现 `build_debug_trace`  
3. 注册 Inspector / Painter 叠层插件  
4. 1–2 个 fixture 的 Trace snapshot  

---

## 9. 落地顺序

| 里程碑 | 交付 | 说明 |
|--------|------|------|
| **T1** | hierarchical trace 投影 + CLI `debug-layout` | 先有唯一现核的真实 trace；JSON 序列化约定从中长出 |
| **T2** | 最小检视页（或 `overlay.svg` 生成） | hier 叠层可演示 |
| **T3** | common 点选 / hash / showcase 链 | 可用性 |
| **T4** | 信封类型与插件壳上提为跨核抽象 | **第二个核**（tree/sequence/circular）落地后才启动；从两份具体 trace 提炼公共部分 |

抽象后置：不为尚不存在的核预钉跨核契约。

算法能力（如 hier Channel）仍跟各核 architecture 里程碑；Trace 只投影已有事实。

---

## 10. 反模式

1. 把所有核的字段摊平成一个巨型无标签 JSON  
2. UI / painter 按 `profile: flowchart` 分支  
3. 产品 `render_svg` 读取 Trace  
4. 未实现的 extension 段画「示意」几何  
5. dummy / 生命线决策写进产品 `nodes` 冒充作者实体  
6. 每个新核复制一套检视器页面  

---

## 11. 开放问题

1. Overlay 由 Rust 还是前端按 Trace 绘制？→ 首期前端或单一 overlay SVG；插件化后可混用。  
2. Router 独立调试是否共用信封？→ 可另设 `RouterDebugTrace` 或 `extension.kind` 外的并列文件；**不**塞进 layout extension 假装是布局决策。  
3. 信封类型何时上提 `plotgram-model` / engine-api？→ T4：第二个核消费之后（与 mvp-scope §0.1 不改 trait 的纪律一致）。

---

## 12. 参考

- [write-authority.md](write-authority.md)  
- [hierarchical/architecture.md](hierarchical/architecture.md)  
- [hierarchical/debug-profile.md](hierarchical/debug-profile.md)  
- [ADR-009](../adr/009-layout-result-decorations.md) · [ADR-007](../adr/007-layout-facts-llm-channel.md) · [ADR-001](../adr/001-diagram-type-not-in-engine.md)  
- Showcase 三层模型：[../../showcase/README.md](../../showcase/README.md)（layout / facet / role，与 Trace 正交）

---

## 13. 摘要

| 问题 | 答案 |
|------|------|
| 面向谁？ | **所有 layout 核**；hier 只是首个 extension |
| 如何扩展？ | `extension.kind == 实现规范名` + 叠层插件 |
| render？ | 产品路径零侵入（trace 走重跑收集） |
| 下一步？ | T1 hier 投影 + CLI；跨核抽象后置到 T4 |
