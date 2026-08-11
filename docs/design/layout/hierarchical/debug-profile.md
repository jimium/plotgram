# Hierarchical · DebugTrace 扩展剖面

> 状态：现行（依赖父契约）  
> 父页：**跨核** [debug-inspector.md](../debug-inspector.md)  
> 算法真源：[architecture.md](architecture.md) · [phases/contracts-and-ir.md](phases/contracts-and-ir.md)  
> 日期：2026-08-03

本文只钉 **`extension.kind == "hierarchical"`** 的字段与叠层；信封、UI 壳、CLI、反模式见父页。

旧路径 `hierarchical/debug-inspector.md` 已迁出为跨核文档，避免把通用检视器误绑在单核上。

---

## 1. 在信封中的位置

```text
LayoutDebugTrace {
  layout: "hierarchical"        # 别名 fixture 下如实记 "architecture"（父页 §5.1）
  common: { nodes, edges, groups, … }
  extension: {
    kind: "hierarchical"        # 规范名，恒定
    …本页字段
  }
}
```

投影器**重跑**与产品相同的管线，沿途收集 `PlanGraph` / ports / metric；**不**改决策，不复用产品 layout 的残留（父页 §4.1）。本页所有几何字段（frame / center / point / main_band 等）统一 **physical** 空间，由投影器经 `orient.rs` 转换后导出，trace 中不出现 canonical 值（父页 §5.4）。

---

## 2. HierarchicalExtension

```text
HierarchicalExtension {
  elems: ElemDebug[]
  layers: LayerDebug[]
  edge_plans: HierEdgeDebug[]
  ports: PortDebug[]
  channels: ChannelDebug?      # 未实现 → null，notes 说明
  metrics: MetricDebug?
  partition_bands: PartitionBandDebug[]   # PG-2；未消费 partition → 不发键
}
```

### 2.1 Elem

```text
ElemDebug {
  key: ElemKeyDebug
  rank: u32
  order: u32
  group_path: string[]
  frame: Rect?
  center: Point?
  kind_tags: string[]          # ["real"] | ["virtual","long-edge-dummy"] …
}

ElemKeyDebug =
  { type: "real", id: NodeId }
  | { type: "virtual", owner_edge: EdgeId, kind: VirtualKind, ordinal: u32 }

VirtualKind =
  "long-edge" | "port-ns" | "group-boundary" | "partition-boundary" | "label"
```

稳定身份对齐 [contracts-and-ir §3](phases/contracts-and-ir.md)；禁止 dense `usize` 进 key。

首期至少支持 `long-edge`；其余随 Plan 能力增长。

### 2.2 Layer

```text
LayerDebug {
  rank: u32
  main_band: { start: f64, end: f64 }?
  elem_keys: ElemKeyDebug[]    # order 序
}
```

### 2.3 Edge plan

```text
HierEdgeDebug {
  edge_id: EdgeId
  original: { source: NodeId, target: NodeId }
  working:  { source: NodeId, target: NodeId }
  reversed: bool
  segments: { ordinal: u32, from: ElemKeyDebug, to: ElemKeyDebug }[]
  dummy_chain: ElemKeyDebug[]
  note: string?                  # 自环钉死 "self-loop"；其余情况省略
}
```

自环不进 rank/order/plan：`segments` / `dummy_chain` 为空数组、`note = "self-loop"`、`reversed = false`；UI 显示其 common 桩几何，不报「缺 plan」（父页 D6）。

最终 path 优先放在 `common.edges[].path`，避免双真源；需要自足单文件时可冗余复制并注明。`DeferToRouter` 时 `path` 为 null 属合法状态。

### 2.4 Port

```text
PortDebug {
  edge_id: EdgeId
  end: "source" | "target"     # original 语义端
  node: NodeId
  side: "N"|"S"|"E"|"W"
  slot: u32?
  point: Point?
  constraint: "free" | "fixed"  # 钉死词表，与现行二档 PortConstraint 对齐；禁自由字符串
}
```

### 2.5 Channel / Metric

```text
ChannelDebug {
  status: "absent" | "partial" | "present"
  reason?: string
  tracks: …?
  gates: …?
}

MetricDebug {
  node_gap: f64
  layer_gap: f64
  # 有 ideal / 不可行摘要再加字段
}

PartitionBandDebug {           # 已消费列带（partition-grid.md PG-2）
  column: string               # 声明序 = 几何左→右（TB）
  band: { start: f64, end: f64 }  # physical cross-axis 区间
  empty?: bool                 # 无成员列（最小宽保留），false 不发
}
```

MVP：`channels = null` 或 `{ status: "absent", reason: "…" }`，与 [mvp-scope](notes/2026-08-02-mvp-scope.md) 一致。

组框：若仍由 finalize bbox 反推，`common.groups[].frame_source = "finalize-bbox"`。

---

## 3. 叠层插件（注册到共用壳）

| 层 ID | 内容 | 首期 |
|-------|------|------|
| `ranks` | 层色带 + rank | ✅ |
| `order` | 层内序标 | ✅ |
| `dummies` | virtual + 链高亮 | ✅ |
| `reversed` | FAS 反向边 | ✅ |
| `ports` | 端口记号 | ✅ |
| `segments` | segment 对照 path | 可选 |
| `channels` | track / gate | ❌ 等 M3 |

Common 层 `product` / `groups` 由壳提供，本核不重复注册。

---

## 4. 与算法里程碑

| 字段 / 叠层 | 依赖 |
|-------------|------|
| rank / order / dummy / reversed / ports | 现行 MVP |
| 组框 metric 真源 | M4 |
| channels | M3 |
| partition band（extension.partition_bands） | PG-2 已落地 |

---

## 5. 验收（hier provider）

1. 小 fixture Trace snapshot 稳定。  
2. 无 dense index 进 `ElemKeyDebug`。  
3. 开关 Trace 后默认产品 SVG 字节不变。  
4. `extension.kind == "hierarchical"`；`layout` 如实记录注册名。  
5. Channel 未实现时不出现假 track 几何。  
6. 同一输入两次运行 trace 逐字节一致（确定性）。  
7. `layout: architecture` 别名 fixture：`layout == "architecture"` 且 `kind == "hierarchical"`，叠层插件正常装载。  
8. 自环边：common 中可见、edge_plans 中有 `note = "self-loop"` 占位，无假 segments。

---

## 6. 参考

- 父契约：[../debug-inspector.md](../debug-inspector.md)  
- [architecture.md](architecture.md) · [mvp-scope](notes/2026-08-02-mvp-scope.md)  
- Atlas Plan：[atlas-reference/plan-ir-diff-fingerprint.md](atlas-reference/plan-ir-diff-fingerprint.md)
