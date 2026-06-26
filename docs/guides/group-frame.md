# Group Frame 使用与调节指南

Group Frame（L1）控制架构图、流程图等含 **group** 图表的**组间宏观几何**：排列方向、同级等宽、交叉轴对齐、间距、边框共线与像素量化。

> 设计规格：[group-frame-spec.md](../已经实现的方案/group-frame-spec.md)  
> DSL 语法：[dsl-writing-manual.md §6.6](../specs/dsl/dsl-writing-manual.md#66-group_frame-统一配置块推荐)  
> 实现：`crates/drawify-core/src/layout/group_frame/`

---

## 何时使用

- 架构图顶层 group 宽度参差不齐，希望形成**等宽阶段条带**
- 泳道 / 分层图需要**左缘对齐**、**边框共线**
- 固定版式（如上二下一）需要**矩阵网格**排列
- 与 **组内** `layout:` hint 配合，同时规整组间与组内

Group Frame 在**主布局算法之后**执行（路由前、路由后各会幂等恢复一次），通过 DSL `config` 块声明即可，无需改 Rust 代码。

---

## 三层 Frame 模型（速查）

```text
L1  Group Frame   — 组间：track 等宽、cross 对齐、gap、border（本指南）
L2  Intra Frame   — 组内：group { layout: horizontal | vertical | fan-out | auto }
L3  Node Frame    — 节点：grid_snap（rank/layer 对齐 + 8px 量化）
```

调节「group 框是否对齐」用 **L1**；调节「group 里节点怎么排」用 **L2**。

---

## 推荐写法：`group_frame` 配置块

旧属性 `group_sizing`、`group_arrangement`、`group_gap`、`group_align` 仍可用（语法糖），新图优先写 `group_frame`：

```drawify
diagram architecture {
    config {
        group_frame: stack {
            axis: horizontal    // 架构图默认：水平条带语义
            track: equal        // 同级 group 等宽（拉齐到最宽者）
            cross: start        // 左缘对齐（架构图默认）
            gap: 50             // 组间净间距（像素）
            border: shared      // 同级边框共线
        }
    }
    // ...
}
```

### `stack` 选项说明

| 选项 | 类型 | 常用值 | 作用 |
|------|------|--------|------|
| `axis` | atom | `horizontal` / `vertical` | 堆叠主轴（架构图多为 `horizontal`） |
| `track` | atom / number | `fit`, `equal`, `uniform`, `320` | 同级 track 尺寸：`equal` 等宽；数字为固定像素宽 |
| `cross` | atom | `start`, `center`, `end` | 交叉轴对齐；`Equal` 时组内内容在 track 内居中 |
| `gap` | number | `40`～`80` | 组间间距 |
| `border` | atom | `none`, `shared` | 同级边框共线（左/上优先） |
| `snap` | bool / number | `true`, `8` | 组框 8px 量化（可与 diagram `snap:` 联动） |

语法糖对照：

| 旧属性 | `group_frame` 等价 |
|--------|-------------------|
| `group_sizing: uniform` | `track: equal` |
| `group_arrangement: horizontal` | `axis: horizontal` |
| `group_gap: 60` | `gap: 60` |
| `group_align: left` | `cross: start` |

---

## 架构图典型配方

### 1. 流水线 / 分层（四层等宽条带）

四个顶层 group 沿数据流上下（或左右）排列，希望**每层外框等宽**：

```drawify
config {
    group_frame: stack {
        axis: horizontal
        track: equal
        cross: start
        gap: 50
        border: shared
    }
}
```

**参考样例**：[`showcase/architecture/c.ai-agent-docops-pipeline.dfy`](../../showcase/architecture/c.ai-agent-docops-pipeline.dfy)、[`n.data-pipeline.dfy`](../../showcase/architecture/n.data-pipeline.dfy)（使用 `group_sizing: uniform` 语法糖）。

拓扑由 **relation** 决定层级；`track: equal` 负责把**同级**顶层 group 拉成相同宽度，组内节点水平居中。

### 2. 微服务三层（网关 / 服务 / 数据）

```drawify
config {
    group_frame: stack {
        axis: horizontal
        track: equal
        cross: start
        gap: 50
        border: shared
    }
    edge_routing: orthogonal { bundling: 1.0 }
}
```

见 [dsl-writing-manual §8.3](../specs/dsl/dsl-writing-manual.md#83-架构图分层服务使用-group_frame)。

### 3. 仅贴合内容（默认）

不写 `group_frame` 或 `track: fit`：每个 group 宽度随内容，适合组内节点数差异大的草图。

---

## 组内排版（L2）

`group_frame` 不替代组内 hint。按组声明：

```drawify
group agent_runtime "Agent 运行时" {
    layout: fan-out      // 枢纽扇出：orchestrator 居中，其余分布两侧
    ...
}

group drawify_stack "Drawify 栈" {
    layout: horizontal   // 一行横排
    ...
}
```

| `layout` | 适用 |
|----------|------|
| `auto` | 默认，按节点数推断 |
| `horizontal` | 3～5 个同级组件横排 |
| `vertical` | 存储层、短链竖排 |
| `fan-out` | 单一枢纽连多个下游（Agent / 网关） |

---

## 矩阵排列：`group_frame: matrix`

需要**显式网格**（如上排 2 个、下排 1 个）时使用：

```drawify
config {
    group_frame: matrix {
        cols: 2
        rows: 2
        track: equal
        gap: 48
        cross: center
    }
}
```

- 顶层 group 按布局后的 `(y, x)` **行优先**填入网格
- `cols` / `rows` 可只写一个，另一个自动 `ceil(n/…)`
- **限制**：下排仅 1 个 group 时占**左格**，暂不支持跨列合并（colspan）

---

## 架构图内置行为（无需 DSL）

| 行为 | 说明 |
|------|------|
| 单 group 行居中 | 某一层只有 1 个顶层 group 时，自动水平居中（`center_single_group_rows`） |
| 默认左缘对齐 | 未写 `group_frame` 时 architecture 使用 `cross: start` |
| 嵌套 group | 每个 parent 下的子 group 集合单独跑一遍 L1（同一 `GroupFrameSpec`） |
| Pin 保护 | `layout intent` 中 Pin 的节点在 Equal 拉宽时不会被平移 |

---

## 调节步骤（实操）

1. **先定拓扑**：用 relation 表达分层与数据流；框的形状无法单靠 DSL 覆盖错误拓扑。
2. **开等宽**：`track: equal` 或 `group_sizing: uniform`。
3. **调组内**：按组加 `layout: horizontal | fan-out | …`，减少单组过宽导致全图被撑大。
4. **调间距**：`gap` 加大可减轻边路由拥挤；架构图常用 `48`～`60`。
5. **边框与量化**：`border: shared` + 默认 `snap: true` 让条带更利落。
6. **预览**：`drawify render your.dfy -o out.svg` 或 showcase 批量脚本。

### 常见问题

| 现象 | 可能原因 | 调节 |
|------|----------|------|
| 等宽后某层特别高 | 该组节点多或 `fan-out` 展开 | 拆组、改 `layout`、或减少组内 entity |
| 两层宽度仍不齐 | 两层不在同一 sibling 集合 | Equal 仅拉齐**同一 parent 下**的兄弟；跨层靠内容与居中 |
| 拉宽后节点偏一侧 | 正常：Equal 会居中组内内容 | 检查 `cross`；Pin 节点不会被移动 |
| 矩阵顺序不对 | 格子顺序由布局后坐标决定 | 先调拓扑让大致顺序正确，再开 matrix |

---

## CLI 验证

```bash
# 渲染查看
cargo run -p drawify-cli -- render showcase/architecture/c.ai-agent-docops-pipeline.dfy -o /tmp/out.svg

# 布局质量（可选）
cargo run -p drawify-cli -- lint showcase/architecture/c.ai-agent-docops-pipeline.dfy
```

Layout hints 中可查看 `group_frame` 报告字段（`GroupFrameReport`：是否 equalized、matrix_applied 等），见 [render-pipeline.md](render-pipeline.md)。

---

## 相关文档

- [layout-intent.md](layout-intent.md) — Pin / Align 与 Group Frame 交互
- [layout-lint.md](layout-lint.md) — 组重叠、节点溢出 group 等检查
- [theme-and-style.md](theme-and-style.md) — 组边框线型 `style: dashed` 等
- [architecture 图表规范](../specs/visual-language/diagrams/architecture.md) — 架构图写作约定
