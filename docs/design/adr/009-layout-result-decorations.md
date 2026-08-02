# ADR-009: LayoutResult 派生几何（Decoration）一等契约

> 状态：accepted（需求已立；实现 **planned**）
> 日期：2026-08-02
> 关联：ADR-001、ADR-006、[`write-authority.md`](../layout/write-authority.md)、[`model-boundary.md`](../model-boundary.md)、[`layout/sequence/architecture.md`](../layout/sequence/architecture.md)

## 背景

当前 `LayoutResult` 只有 `nodes` / `edges` / `groups` / `labels` + canvas。这对 flowchart 类「节点框 + 边折线」够用，但不够表达：

- **序列图**：生命线、激活条、穿越缺口（不在 `Graph` 里，却必须占几何）；
- 后续可能的甘特条、专用标注带、其它「布局派生、非业务节点」形体。

v1 的做法是 render 按 `DiagramType::Sequence` **现算**生命线，并用 `LayoutHints.sequence` 旁路传缺口。这会造成：

1. **双写者**：几何决策漏到 render；
2. **图种分支**：违反 ADR-001 精神（render 也不该靠图名发明坐标）；
3. **难测**：坐标只在 SVG 里出现，布局 snapshot 不完整；
4. **难扩展**：每个新图种再开一套 paint 特判。

需要在重建里先钉死 **Layout → Render 的几何契约**：布局写出全部语义几何；渲染只消费、只上色。

## 决策

### 1. `LayoutResult` 增加派生几何通道

```text
LayoutResult
  nodes / edges / groups / labels / canvas   # 现有
  decorations: Vec<Decoration>               # 新增；稳定声明序；缺省空
```

`Decoration` 是**布局派生、不进入 `Graph`** 的可绘制几何。首期枚举（可增，勿用自由 attrs 冒充）：

```text
Decoration =
  Lifeline {
    id,                    # 稳定，如 lifeline:{participant_id}
    participant: NodeId,
    x, y0, y1,             # 轴线几何
    gaps: [y…],            # 可选；穿越消息处缺口中心 y
  }
  Activation {
    id,                    # 稳定，如 activation:{edge_id}:{ordinal}
    lifeline_id,
    frame: Rect,
    depth: u32,
  }
```

后续图种需要新类型时：**扩展枚举 + render 增加画法**，禁止再引入 `hints.<diagram>` 旁路。

### 2. 写权

| 自由度 | 写者 | 禁止 |
|--------|------|------|
| decoration 是否存在、id、几何数值 | **Layout（Metric / Ink）** | Render 推算生命线终点、激活区间 |
| 消息 / 边 path | Layout Ink | Render 改拓扑 |
| 节点 frame | Layout | Render 为「好看」挪框 |
| 颜色、虚线样式、线宽、marker、z 序 | **Render / theme** | Layout 写 SVG 笔触 |
| 形状轮廓（cloud 等） | Render 按 `Node.shape` + frame 画 | Layout 交出云多边形点列（非必须） |

判断句：若修法是「在 render 里再算一段坐标」，先问该几何的 layout 写者是谁。

### 3. Render 消费规则

1. 绘制顺序建议：`groups` → `Lifeline` → `Activation` → `edges` → `nodes` → `labels`（具体 z 可调，但不得改坐标）。
2. **按 decoration 类型分派画法**，不按 `DiagramType` / `profile` 名分支发明几何。
3. `decorations` 为空时，行为与今日通用图一致（flowchart 无回归税）。
4. 未知 decoration 变体（前向兼容）：硬失败或显式跳过并 warning——二选一须在实现时钉死；禁止静默画错。

### 4. 与节点框的关系（序列图）

- 参与者**头部**仍是普通 `NodePlacement`（`shape` 可为 rect/actor 等）。
- **生命线不是** `Node.shape = "lifeline"`；它是挂在头部之下的 `Lifeline` decoration。
- **激活条不是**业务节点；它是 `Activation` decoration。

### 5. 与形状裁剪的关系（cloud 等）

- Layout 对节点提供 **frame（外接矩形）+ 端口决议 + 边 path（可先落在框边）**。
- 可见轮廓在 frame 内由 render 绘制；边端点贴轮廓属于 **Ink/落笔形状裁剪**（目标能力），**不是**把轮廓点列塞进 `Decoration`。
- Decoration 解决的是「Graph 里没有的派生形体」；形状裁剪解决的是「有 shape 的节点边怎么贴边」。

### 6. ADR-001

- Engine / Render **都不**因 `profile: sequence` 分支去猜生命线。
- Sequence 布局核写出 `Lifeline`/`Activation`；其它核写出空 `decorations` 或其它类型。
- 差异来自 **算法产出的几何类型**，不是图种字符串。

### 7. 落地状态

| 项 | 状态 |
|----|------|
| 本文契约 | **accepted** |
| `plotgram-model` 类型 | **planned** |
| Sequence layout 写出 | **planned**（见 sequence architecture） |
| `plotgram-render` 消费 | **planned** |
| 删除 v1 式 diagram-type paint 几何 | 重建达到后禁止回归 |

未实现前：不得用 render 特判冒充已完成；不得再扩大 `LayoutHints` 旁路。

## 含义

| 层 | 影响 |
|----|------|
| **model** | `result` 模块扩展 `Decoration` / `LayoutResult.decorations`；稳定序容器 |
| **engine-api** | `LayoutOutput` 若与 Result 分流，须能携带同等派生几何或在 finalize 合并 |
| **engine** | Sequence（及未来核）Metric/Ink 写 decorations；Hier 默认空 |
| **render** | decoration 绘制通道；禁止按图种算坐标 |
| **测试** | decorations 进 layout JSON snapshot；双跑 bit-identical |
| **文档** | sequence architecture 以本文为契约真源；model-boundary 链到本文 |

## 非目标（首期）

- 把所有 SVG 细节（圆角、dasharray、marker path）塞进 layout
- 用 decoration 替代 `groups` / `labels`
- 为 cloud/菱形引入「轮廓 Decoration」（那是 shape + frame + 裁剪）
- 一次实现甘特/火焰图的全部 decoration 变体
- 向后兼容 v1 `LayoutHints.sequence`

## 备选方案（未采用）

| 方案 | 放弃原因 |
|------|----------|
| Render 按 `layout.name == "sequence"` 自算生命线 | 双写者；图种分支；难测 |
| `Node.shape = "lifeline"` 冒充派生线 | 污染节点语义；激活条仍无着落 |
| 继续 `hints.sequence` 旁路 | 非正式、易膨胀、与 Result 双真源 |
| 派生几何写回 `Graph` | 生命线/激活条不是 DSL 作者实体；违反 model 边界 |
| 仅扩展 `groups` 画生命线 | group 是包含树框，语义不符 |

## 验收清单（改造完成时）

1. `LayoutResult` 含 `decorations`；缺省 `[]` 时现有 SVG 回归通过。
2. Sequence 最小样例：生命线/激活条坐标仅来自 decorations，render 源码无「按 canvas 高度猜线」。
3. 同输入双跑 decorations bit-identical。
4. flowchart 样例 decorations 为空。
5. 引擎与 render 中无新增 `DiagramType` / `profile` 名驱动的几何分支。

## 参考

- [`layout/sequence/architecture.md`](../layout/sequence/architecture.md) §2.2
- [`layout/sequence/phases/message-routing.md`](../layout/sequence/phases/message-routing.md)
- [`reference/yfiles/15-几何与落笔层.md`](../../reference/yfiles/15-几何与落笔层.md)
- [`reference/yfiles/09-yfiles类引擎架构.md`](../../reference/yfiles/09-yfiles类引擎架构.md)（LayoutGraph = 纯几何）
- v1 反面教材：`crates/v1/.../render/paint/sequence.rs` + `SequenceLayoutHints`
