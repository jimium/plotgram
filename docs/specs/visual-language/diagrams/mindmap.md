# 思维导图 (mindmap)

## 定位

以**树形或近似树形**结构展开知识、计划或创意。中心主题向外辐射，层级由深到浅。

**核心问题**：*这个主题如何分解、有哪些分支？*

---

## 适用场景

| ✅ 适合 | ❌ 不适合 |
|---------|----------|
| 项目计划、里程碑拆解 | 有向流程与回环（用 `flowchart`） |
| 技术栈选型、知识体系 | 多方时序交互（用 `sequence`） |
| 产品路线图、头脑风暴 | 组件依赖网络（用 `architecture`） |
| 会议议题、培训大纲 | 状态转换（用 `state`） |

---

## 语法入口

```drawify
diagram mindmap {
    title: "产品规划"

    entity root "产品规划" { type: root }
    entity feature "功能需求" { type: main }
    entity tech "技术方案" { type: main }
    entity mvp "MVP 范围" { type: leaf }

    root -> feature
    root -> tech
    feature -> mvp
}
```

> 省略 `layout` 时默认 **中心辐射**（`radial`）。大纲式单向树可写 `layout: top-to-bottom` 或 `left-to-right`。

> **书写提示**：`start` / `process` / `service` / `database` / `end` 会归一化为 `root` / `branch` / `leaf` 等。推荐直接使用规范 type。

---

## 允许的实体 type

| type | 语义 | 视觉形状 | 别名（自动归一化） |
|------|------|----------|-------------------|
| `root` | 中心主题 | 圆形（深紫、加粗） | `start` → `root` |
| `main` | 一级分支 | 圆角矩形（中紫、加粗） | — |
| `branch` | 中间节点 | 圆角矩形（浅紫） | `process`, `service` → `branch` |
| `leaf` | 叶子节点 | 圆角矩形（最浅紫） | `database`, `end` → `leaf` |

完整列表以 `MINDMAP_ENTITY_TYPES`（`diagram/registry.rs`）为准。

**结构约束**：
- 全图最多 **1 个** `root` 节点（校验 enforced）
- 从 `root` 向外单向展开，避免回环
- 层级深度建议 ≤ 4，过深影响可读性

---

## 关系箭头约定

在思维导图中，relation 表示**父子层级**，不含流程方向语义：

| 箭头 | 含义 | 视觉 |
|------|------|------|
| `->` | 父节点 → 子节点 | 紫色曲线，**无箭头** |
| `-->` | 不使用 | — |
| `<->` | 不使用 | — |

思维导图边默认 `ArrowStyle::None`，以平滑贝塞尔曲线连接，强调树形而非流向。

---

## 布局与视觉默认值

| 属性 | 默认值 | 说明 |
|------|--------|------|
| `layout-algo` | `mindmap` | 思维导图专属布局（中心辐射 / 单向树） |
| `edge-routing` | `bezier` | 平滑曲线辐射 |
| `layout` | `radial`（省略时） | 中心主题居中、一级分支左右交替；亦可 `top-to-bottom` / `left-to-right` |
| 样式方案 | `builtin.clean-light` | 紫色主题节点 |

### 布局方向

| `layout` 值 | 效果 | 适用场景 |
|-------------|------|----------|
| `radial`（**默认**） | `root` 居中，子树向左右交替辐射 | 头脑风暴、知识图谱 |
| `top-to-bottom` | `root` 在上方，树形向下展开 | 培训大纲、目录结构 |
| `left-to-right` | `root` 在左侧，树形向右展开 | 横向路线图 |

---

## 分组 (group)

思维导图**不使用** group — 层级由 `root` → `main` → `branch` → `leaf` 的 type 与 relation 树表达。

---

## 写作规范

1. **有且仅有一个 `root`** — 中心主题。
2. **第一层分支用 `main`** — 与 `branch`/`leaf` 在视觉上区分权重。
3. **叶子节点用 `leaf`** — 不再向下展开的内容。
4. **relation 只表达包含关系** — 不写 `"然后"`、`"调用"` 等流程语义。
5. **控制宽度** — 每个 `main` 下建议 3–7 个直接子节点（米勒定律）。

---

## 实现状态

✅ 布局、渲染器、主题均已实现（`DiagramProfile.implemented = true`）。

✅ 分支配色已纳入 StyleSheet cascade：`branch_palettes` + `{branch.*}` contextual token 在 `materialize_styles` 中统一物化，不再走 prepare 特例路径。详见 [Mindmap 统一主题方案](../../../architecture/mindmap-unified-theming-design.md)。

---

## 示例

| 复杂度 | 路径 | 说明 |
|--------|------|------|
| 简单 | `showcase/mindmap/s.brainstorm.dfy` | 中心 + 三分支 |
| 简单 | `showcase/mindmap/s.root-branches.dfy` | 基础树形 |
| 正常 | `showcase/mindmap/n.tech-stack.dfy` | 技术栈 |
| 复杂 | `showcase/mindmap/c.product-roadmap.dfy` | 产品路线图 |

---

## 导出与导入（规划）

除 SVG / PNG / WebP / ASCII / Scene JSON / Draw.io 外，大纲类 interchange（Markdown、OPML、FreeMind）及 **Markdown 大纲直接出图** 方案见 [Mindmap 大纲类 Interchange 方案](../../../architecture/mindmap-interchange-export-design.md)（§7 Import 通道）。

---

## 参见

- [实体类型标准](../entity-types.md) — 思维导图层级 type
- [Mindmap 大纲类 Interchange 方案](../../../architecture/mindmap-interchange-export-design.md) — 导出/导入、Markdown 大纲 → 图
- [布局算法 — 思维导图](../../../architecture/layout-algorithms.md)
- [流程图](./flowchart.md) — 顺序流 vs 树形展开
- [状态图](./state.md) — 离散状态 vs 知识层级
