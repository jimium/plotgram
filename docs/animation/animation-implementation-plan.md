# Drawify 动画能力落地方案

> 版本：0.2.0 | 状态：实施方案 | 日期：2026-06-24
> 依据：[animation-capability-research.md](./animation-capability-research.md)（研究评估）、[svg-embedding-design-impact.md](./svg-embedding-design-impact.md)（嵌入方式设计约束）、[competitive-strategy.md](../product/competitive-strategy.md) §4.5、[language-spec.md](../specs/language-spec.md)、[export-scene-spec.md](../specs/export-scene-spec.md)、[ast-spec.md](../specs/ast-spec.md)、[diff2/README.md](../../crates/drawify-core/src/diff2/README.md)
> 适用：Drawify Core / CLI / Server / WASM / Studio

---

## 0. 一句话结论

**只做语义动画，不做装饰动画；全面使用 CSS（SVG 内嵌 `<style>`），放弃 SMIL。** 以 `diff2::ChangeSet` 为语义源、以 SVG 元素稳定 ID 为锚点，分四阶段把"改得更精准"变成"肉眼可见的精准"。DSL 仅扩展一个 `steps:` 块，动画参数全部走渲染层，不污染内容语言。CSS 通过 `<defs><style><![CDATA[...]]></style></defs>` 内嵌于 SVG 文件，单文件自包含，双击打开即有动画，零外部依赖。

---

## 1. 用户需求分析

### 1.1 三类真实用户与他们的痛点

| 用户 | 当前痛点 | 动画能解决什么 |
|------|----------|----------------|
| **AI Agent**（直接调用方） | 生成初版后用户说"把认证服务移到右边"，Agent 只能整图重生成，用户难以确认改了什么 | Patch 动画让变更可感知，Agent 不需要解释"我改了哪里" |
| **企业架构师**（Studio 用户） | 架构变更 PR 评审只能截图对比，看不出节点移动轨迹 | Diff 过渡 + Steps 让"变更讲解"变成可播放的演示 |
| **技术文档作者** | 静态图无法表达"先 A 再 B"的时序，只能贴多张图 | Steps 系统让一张图按节奏展开，导出 HTML 即可播放 |

### 1.2 伪需求识别（明确不做）

| 伪需求 | 来源 | 不做的原因 |
|--------|------|------------|
| 节点弹跳/呼吸/发光等装饰动画 | "动画看起来很酷" | 归入"图形美观"维度，Mermaid 追平成本低，不构成壁垒（[competitive-strategy.md](../product/competitive-strategy.md) §2.1） |
| 交互式图探索（拖拽/缩放/折叠） | "像 Cytoscape 一样" | 与产品定位冲突——Drawify 是静态导出，不是交互探索器（[cytoscape-js-research.md](../architecture/参考资料/cytoscape-js-research.md)） |
| DSL 中写 `animate: pulse` 等节点级动画属性 | "用户想精确控制" | 动画是渲染关注点，污染 DSL 后破坏"语义优先"原则，且 Agent 生成成本飙升 |
| GIF/MP4 光栅化导出（首发） | "PPT 要嵌入" | 依赖 Playwright + ffmpeg 重依赖，且失真；先用 HTML 导出覆盖 90% 场景 |

### 1.3 必须满足的硬约束

- **零 JS 依赖的纯 SVG 也能工作**——导出的 SVG 双击打开即有动画（CSS `<style>` 内嵌 `<defs>`，非外部 HTML 依赖）
- **动画可被完全关闭**——`compat_mode: Static` 或 `prefers-reduced-motion` 时退化为静态图
- **Agent 零参数**——Patch 动画由渲染器自动生成，用户/Agent 不需要指定时长、缓动
- **确定性**——同一输入多次渲染结果一致（遵守 [AGENTS.md](../../AGENTS.md) §2，动画编排不得依赖 HashMap 迭代序）
- **CSS 是唯一动画技术**——全面放弃 SMIL，所有动画用 CSS `@keyframes`/`transition` 实现，样式内嵌于 SVG `<defs><style>`

---

## 2. 项目增值点分析

### 2.1 战略增值：放大核心壁垒

Drawify 的核心壁垒是**语义微调**（[competitive-strategy.md](../product/competitive-strategy.md) §4）。动画本身不是壁垒，但**语义动画是壁垒的视觉放大器**：

```
没有动画：Patch 生效 → 新图替换旧图 → 用户自己对比哪里变了
有了动画：Patch 生效 → auth 节点平滑滑到新位置 → 用户零成本感知变更
```

这个差异在**连续交互**（Agent 迭代 5 次微调）场景下被放大，直接强化"改得更精准"的可感知性。

### 2.2 商业增值：企业场景的差异化

| 企业场景 | 当前方案 | Drawify + 动画后 |
|----------|----------|-------------------|
| 架构变更 PR 评审 | 截图对比 / 文本 diff | 语义 Diff + 过渡动画，一眼看清改了什么 |
| 变更讲解会议 | 贴多张图手动翻页 | Steps 演示，按节奏播放演变 |
| 培训材料 | 静态图 + 文字说明 | Steps HTML 导出，可交互回看 |
| 合规归档 | 截图存档 | 带 ChangeSet 的 SVG，可追溯每次变更 |

D2 已用"唯一能从文本生成动画图的语言"占据开发者心智（[research §2.3](./animation-capability-research.md)）。Drawify 的差异化在于：**帧间有语义插值**（D2 是硬切换），且动画**零参数自动生成**。

### 2.3 技术增值：盘活现有基础设施

动画能力让以下已有模块的价值翻倍：

| 已有模块 | 当前用途 | 动画加持后 |
|----------|----------|------------|
| `diff2::ChangeSet` | PR 审阅、Agent 增量改图 | 直接驱动过渡动画，语义级精准 |
| `ExportScene`（entity.id / edge.index） | 多格式导出的稳定契约 | 成为动画元素的唯一锚点 |
| `Layout Intent Overlay`（规划中） | 局部位置微调 | 微调过程可视化，用户看到节点滑动 |

### 2.4 不增值的领域（明确放弃）

- **美观竞争**：不靠动画让图"更好看"，那是主题/渲染风格的事
- **生态传播**：动画不解决 GitHub/Notion 集成问题，那是 Mermaid 的护城河
- **交互探索**：不做拖拽/折叠/缩放，与静态导出定位冲突

---

## 3. 代码实现可行性评估

基于对 `crates/drawify-core/src` 的实际代码核查（非文档转述）：

### 3.1 现状核查

| 维度 | 实际代码现状 | 对动画的影响 |
|------|--------------|--------------|
| SVG 生成方式 | `render/paint/scene_svg.rs::encode()` 纯字符串拼接 | ✅ 易插入 id 属性、`<style>` 块与 CSS class |
| SVG 元素身份 | **核查确认：当前 `paint_export_node` / `paint_export_edge` 未注入任何 `id` 属性** | ❌ 阻塞，必须先补 |
| 节点身份锚点 | `ExportNode.entity.id: Identifier` 已存在 | ✅ 可直接作为 id 来源 |
| 边身份锚点 | `ExportEdge.index: usize`（与 `relations[index]` 对齐） | ✅ 可直接作为 id 来源 |
| 语义 Diff | `diff2::diff(old, new) -> ChangeSet` 已实现，覆盖 Add/Remove/Modify × Entity/Relation/Group/StyleDecl | ✅ 天然映射动画语义 |
| Patch 触发重渲染 | ❌ 当前 Patch 产出 RawDiagram 后需手动重跑管线 | ⚠️ 需新增动画编排入口 |
| 渲染参数 | `RenderRequest` 已有结构（`render/request.rs`） | ✅ 可加 `animation` 字段 |
| Steps 系统 | ❌ 完全不存在，AST `Diagram` 无 `steps` 字段 | ❌ 需扩展 AST + DSL + 渲染 |
| 交互事件绑定 | Studio 通过 `dangerouslySetInnerHTML` 注入 SVG | ⚠️ 仅影响 Studio 端 JS 增强，不影响导出 |
| 导出格式 | `encode/mod.rs` 已有 SVG/PNG/WebP/ASCII/JSON | ⚠️ 需新增 HTML 动画编码器 |

### 3.2 关键技术决策（已锁定）

| 决策点 | 选择 | 理由 |
|--------|------|------|
| Patch 动画技术 | **SVG 内嵌 CSS `@keyframes` + class 注入** | 导出友好，纯 SVG 自包含；CSS 全浏览器硬件加速；`transform-box:fill-box` 支持节点缩放/平移；可访问性一行 `@media` 搞定；SMIL 在 `<img>` 下按钮失效且路径插值有硬伤（详见 [svg-embedding-design-impact.md §3.1](./svg-embedding-design-impact.md)） |
| 边路径变形 | **交叉淡入淡出**（旧路径 opacity 1→0 + 新路径 opacity 0→1） | 规避 SMIL `d` 属性插值的点数对齐难题，CSS opacity 动画即可实现，视觉效果平滑 |
| 边元素 ID | **`edge-{relation.id}`**（永久稳定 ID，非 index） | `edge-{index}` 在中间插入/删除边后会错位，导致 Patch 动画把"移动"误判为"删除+新增"；需在 AST 层给 Relation 分配永久 id（详见 [svg-embedding-design-impact.md §5.3](./svg-embedding-design-impact.md)） |
| Steps 语法 | **完整图 Step（E1）为主，增量 Step（E2）为语法糖** | E1 容错性好，每步独立；E2 在解析层累积转 E1，对 Agent 和人类更友好 |
| 导出格式 | **两种格式严格分层**：纯 SVG（自播放，无按钮）+ HTML（Steps 交互，内联 SVG） | `<img>` 完全隔离交互是平台限制无法绕过，SVG 按钮在 GitHub 下失效，必须分层（详见 [svg-embedding-design-impact.md §4](./svg-embedding-design-impact.md)） |
| DSL 动画语法 | **仅 `steps:` 块** | 动画参数属渲染层，不污染 DSL（[research §5.2.1](./animation-capability-research.md)） |
| JS 控制器 | **自研轻量播放器 < 2KB** | 仅用于 Steps 时序（class 切换），核心动画全在 CSS 中，不做 JS 属性插值，避免引入 GSAP/WAAPI 复杂度 |
| 兼容性降级 | **三级 `CompatMode`**：Modern / Safe / Static | Modern 用 `transform-box:fill-box` 做缩放动画；Safe 仅 opacity+translate；Static 完全无动画 |
| CSS 样式位置 | **SVG `<defs><style><![CDATA[...]]></style></defs>`** | 单文件自包含，双击 `.svg` 即有动画，`<img>` 嵌入也能播放，CSS 不是 HTML 专属 |
| Remove 动画策略 | **双方案**：HTML Steps 用双图层叠加；SVG Patch 用保留元素 + `.dfy-exit` class | DOM 中不存在的元素无法播放动画（详见 [svg-embedding-design-impact.md §7](./svg-embedding-design-impact.md)） |

### 3.3 风险与缓解

| 风险 | 概率 | 影响 | 缓解 |
|------|------|------|------|
| 旧浏览器 `transform-box: fill-box` 不兼容 | 低 | 低 | `CompatMode::Safe` 降级为仅 opacity+translate 动画，不使用 scale |
| Steps 系统工作量超预期 | 中 | 高 | 分阶段交付：先 Patch 动画（阶段一）再 Steps（阶段三），Steps 复用 Patch 动画 CSS class |
| 大图（>100 节点）动画卡顿 | 中 | 中 | 仅动画 ChangeSet 涉及元素；大图降级为瞬时应用无过渡；CSS transform/opacity 走 GPU 合成层 |
| DSL `steps` 与现有解析器冲突 | 低 | 中 | `steps` 作为保留关键字，仅在 `diagram_body` 内识别，不影响 entity/relation 解析 |
| Studio `dangerouslySetInnerHTML` 全量替换导致动画期间重渲染 | 中 | 中 | 动画期间用 `useDeferredValue` 暂停 React 重渲染；JS 切 class 而非全量替换 SVG |
| Remove 动画需保留旧节点 | 中 | 低 | 纯 SVG 导出时旧节点加 `.dfy-exit` class 淡出后 `visibility:hidden`；HTML/Studio 用 `animationend` 事件移除 DOM |

### 3.4 可行性结论

**无架构性阻碍。** 语义层（diff2、ExportScene 身份锚点）支持度高，渲染层需补齐元素 ID + 动画模块 + Steps 系统。改造路径清晰，工作量集中在阶段一（基础设施）和阶段三（Steps）。

---

## 4. 产品设计

### 4.1 产品形态总览

```
┌─────────────────────────────────────────────────────────────┐
│                    Drawify 动画产品矩阵                       │
├──────────────────┬──────────────────┬───────────────────────┤
│  Patch 动画       │  参数化指令       │  Steps 动画系统        │
│  (diff2 驱动)     │  (渲染参数)       │  (DSL steps: 块)       │
├──────────────────┼──────────────────┼───────────────────────┤
│  Agent 微调时     │  状态图激活节点   │  分步演示流程演变       │
│  自动触发         │  架构图高亮路径   │  Agent 生成系列步骤图    │
│                  │  边数据流方向     │  导出 HTML/SVG 播放器   │
├──────────────────┼──────────────────┼───────────────────────┤
│  输出：带 CSS     │  输出：带 class  │  输出：自包含 HTML      │
│  动画的 SVG       │  的 SVG          │  (SVG 帧+CSS+JS 播放器) │
│  (CSS内嵌<defs>)  │  (CSS内嵌<defs>) │  纯 SVG 版:CSS:target  │
└──────────────────┴──────────────────┴───────────────────────┘
```

### 4.2 三大产品能力详述

#### 4.2.1 Patch 动画（核心，P1）

**触发时机**：Agent 应用 Patch / 用户在 Studio 修改 DSL / PR 评审对比两个版本。

**用户感知**（全部为 CSS 动画）：
- 新增节点 → 淡入 + 缩放 0→1（`@keyframes dfy-enter`：`opacity:0→1; transform:scale(0→1)`，`transform-box:fill-box; transform-origin:center`）
- 删除节点 → 淡出 + 缩放 1→0（`@keyframes dfy-exit`，动画结束后 `visibility:hidden`）
- 节点位置变更 → 平移过渡（CSS `transition: transform`，初始设 `transform:translate(oldX,oldY)`，下一帧移除触发过渡）
- 节点属性变更 → 颜色/样式平滑插值（CSS `transition: fill/stroke/filter`）
- 边新增/删除 → 淡入淡出（`opacity:0→1` / `opacity:1→0`）
- 边路径变更 → 交叉淡入淡出（旧路径淡出 + 新路径淡入，不做 `d` 属性插值）

**零参数承诺**：动画时长、缓动、延迟全部由 `AnimationSpec` 默认值决定，用户和 Agent 不需要指定。

#### 4.2.2 参数化动画指令（P1）

**触发时机**：渲染时通过参数指定，不修改 DSL 源码。

**指令集**：

| 指令 | 语义 | 视觉效果 |
|------|------|----------|
| `highlight(node_id)` | 高亮指定节点 | 边框加粗 + 颜色强调 |
| `activate(node_id)` | 激活态（状态图） | 填充色变为激活色 |
| `flow(edge_index)` | 触发边数据流 | `stroke-dashoffset` 循环动画 |
| `focus(node_id)` | 聚焦 | 目标节点正常，其余元素 opacity 0.3 |

**Agent 友好**：Agent 通过渲染参数控制，而非修改 DSL。例如"激活状态图的节点 X"= 渲染时传 `activate: ["X"]`。

#### 4.2.3 Steps 动画系统（P1）

**触发时机**：DSL 中声明 `steps:` 块，渲染为可播放的多帧动画。

**产品定位**：类似 PPT 的分步播放，但帧间有 Patch 动画过渡（优于 D2 的硬切换）。

**核心场景**：
- Agent 生成一系列步骤图演示流程演变（如"用户请求 → 鉴权 → 响应"）
- 架构师讲解变更演进（如"v1 单体 → v2 微服务 → v3 服务网格"）
- 技术文档作者表达时序（如"先初始化，再加载，最后渲染"）

### 4.3 降级与可访问性

**三级兼容模式**（`RenderRequest.compat_mode`）：

| 模式 | 行为 | 适用场景 |
|------|------|----------|
| `Modern`（默认） | CSS `@keyframes` + `transition`，含 `transform-box:fill-box` 缩放动画 | 现代浏览器、Studio、HTML 导出 |
| `Safe` | 仅 CSS `transition`，不使用 `transform-box:fill-box` 缩放动画（用 opacity + translate 替代） | 旧浏览器、企业内网（Safari <13.1） |
| `Static` | 无动画，纯静态 SVG | 静态文档、PDF 嵌入、性能敏感场景 |

**可访问性**：所有导出的 SVG/HTML 必须在内嵌 `<style>` 中包含：

```css
@media (prefers-reduced-motion: reduce) {
  * { animation: none !important; transition: none !important; }
}
```

CSS 方案的优势在于一行全局媒体查询即可禁用所有动画，无需遍历禁用 SMIL 元素。

---

## 5. 用户如何使用

### 5.1 Agent 用户（API 调用方）

#### 场景 A：Agent 微调时自动产生 Patch 动画

```bash
# Agent 生成初版
drawify render v1.dfy -o v1.svg

# 用户说"把认证服务移到右边"，Agent 生成 v2 + ChangeSet
drawify render --transition v1.dfy v2.dfy \
  --changes changeset.json \
  --duration 400 \
  -o transition.svg
```

输出 `transition.svg` 是自包含的，双击打开即播放过渡动画。Agent 不需要控制动画参数（duration 有默认值）。

#### 场景 B：Agent 生成 Steps 演示

Agent 一次性生成带 `steps:` 的 DSL，渲染为 HTML：

```bash
drawify render steps.dfy --format html-animation -o demo.html
```

`demo.html` 可直接发给用户，浏览器打开即可播放。

#### 场景 C：Agent 用参数化指令高亮

```bash
drawify render arch.dfy \
  --animate highlight:auth_service,flow:edge-3 \
  -o highlighted.svg
```

### 5.2 Server HTTP API 调用

```
POST /api/v1/render
{
  "source": "diagram flowchart { A -> B }",
  "format": "svg",
  "animation": {
    "kind": { "Transition": { "changes": { ... } } },
    "duration_ms": 400,
    "easing": "EaseInOut"
  },
  "compat_mode": "Modern"
}
```

```
POST /api/v1/render/steps
{
  "source": "diagram flowchart { steps: { ... } }",
  "format": "html",
  "step_duration_ms": 800,
  "autoplay": false
}
```

### 5.3 WASM / Studio 用户

```typescript
// Studio 中调用 WASM
const transitionSvg = drawify.renderWithTransition({
  oldSource, newSource, changes, opts: { duration_ms: 400 }
});

const stepsHtml = drawify.renderSteps({
  source, opts: { step_duration_ms: 800, autoplay: false }
});
```

Studio 端额外提供：
- 步骤时间轴（可拖拽定位）
- 过渡速度调节滑块
- 单步预览
- 导出当前帧

### 5.4 DSL 作者（人类用户）

#### 写一个 Steps 演示

```drawify
diagram flowchart {
    title: "用户认证流程"
    config { direction: left-to-right }

    steps: {
        "1. 初始请求" as s1 {
            entity client "客户端" { type: client }
            entity api "API" { type: service }
            client -> api "GET /resource"
        }
        "2. 鉴权" as s2 {
            entity client "客户端" { type: client }
            entity api "API" { type: service }
            entity auth "认证服务" { type: service }
            client -> api "GET /resource"
            api -> auth "verify token"
            auth --> api "valid"
        }
        "3. 响应" as s3 {
            entity client "客户端" { type: client }
            entity api "API" { type: service }
            entity auth "认证服务" { type: service }
            client -> api "GET /resource"
            api -> auth "verify token"
            auth --> api "valid"
            api --> client "200 OK"
        }
    }
}
```

渲染命令：

```bash
drawify render auth-flow.dfy --format html-animation -o auth-flow.html
```

打开 `auth-flow.html` 即可看到三步演示，帧间自动计算 diff 并播放过渡。

---

## 6. DSL 扩展改造

### 6.1 设计原则（不可妥协）

1. **动画不污染核心 DSL**——DSL 描述"是什么"，不描述"怎么动"
2. **Steps 是唯一例外**——Steps 是内容结构（多帧图），属于 DSL 范畴
3. **参数化指令走渲染参数**——通过 CLI flag / API 参数 / Studio UI 传入，不进 DSL
4. **不引入 `animate` / `transition` / `keyframe` 关键字**——避免 DSL 走向 CSS 选择器复杂度

### 6.2 语法扩展：仅 `steps:` 块

#### 6.2.1 EBNF 扩展

```
<diagram_body> ::= ( <diagram_attribute> | <config_block>
                  | <entity_declaration> | <relation_declaration>
                  | <group_declaration> | <style_decl>
                  | <steps_block> )*           // ← 新增

<steps_block>   ::= "steps" ":" "{"
                      <step_declaration>+
                    "}"

<step_declaration> ::= <string> [ "as" <identifier> ] "{"
                        <diagram_body>            // 复用现有规则，但禁止嵌套 steps
                      "}"
```

#### 6.2.2 保留字新增

在 [language-spec.md §12](../specs/language-spec.md) 保留字列表中新增：

```
steps, as    // 词法关键字
```

`as` 仅在 `steps:` 块内的 step 声明中作为关键字，其他位置仍可用作普通 identifier（不冲突，因为 entity/group id 不允许是保留字，但 `as` 当前未在保留字列表中——需新增并文档化其限定作用域）。

> **实现注意**：为避免破坏现有 identifier 解析，`as` 采用**上下文敏感**处理——仅在 `<string> as <identifier>` 模式下识别为关键字。这违反"禁止上下文相关解析"原则的特例，需在 language-spec.md 显式声明。

#### 6.2.3 语义约束新增

在 [language-spec.md §13](../specs/language-spec.md) 语义约束中新增：

| 编号 | 约束 | 错误码 |
|------|------|--------|
| S17 | `steps:` 块最多出现一次 | E005 |
| S18 | `steps:` 块内至少包含 2 个 step（单步无意义） | W008 |
| S19 | step 内的 `<diagram_body>` 禁止嵌套 `steps:` 块 | E005 |
| S20 | step 的 `as <id>` 必须在 steps 范围内唯一 | E002 |
| S21 | step 内声明的 entity ID 在全局唯一（跨 step 也唯一，保证 diff 身份对齐） | E002 |

#### 6.2.4 解析规则

- `steps` 是 `diagram_body` 内的保留关键字
- 每个 Step 是一个完整的子 Diagram（共享外层 `diagram_type` 与 `config`）
- Step 间过渡由渲染器自动计算（`diff2::diff(step[i], step[i+1])`）
- Step 的 `as <id>` 可选，用于参数化指令引用（如 `highlight` 指定某 step 的节点）

### 6.3 AST 扩展

在 [ast-spec.md §2.1](../specs/ast-spec.md) `Diagram` 结构中新增字段：

```rust
pub struct Diagram {
    pub diagram_type: DiagramType,
    pub attributes: Vec<DiagramAttribute>,
    pub entities: Vec<Entity>,
    pub relations: Vec<Relation>,
    pub groups: Vec<Group>,
    pub style_decls: Vec<StyleDecl>,
    pub source_info: SourceInfo,

    /// 新增：分步演示声明
    /// None 表示无 steps 块（普通单帧图）
    /// Some(Vec<Step>) 表示多帧演示
    pub steps: Option<Vec<Step>>,
}

/// 单个演示步骤
pub struct Step {
    /// 步骤标题（人类可读）
    pub label: String,
    /// 步骤 ID（可选，用于参数化指令引用）
    pub id: Option<Identifier>,
    /// 该步骤的完整子 Diagram（共享外层 diagram_type 与 config）
    pub diagram: Box<Diagram>,
}
```

**设计要点**：
- `steps: Option<...>`——保持向后兼容（None = 普通图），但项目无向后兼容约束（[AGENTS.md §1](../../AGENTS.md)），可直接用 `Vec<Step>` 空数组表示无 steps
- `Step.diagram: Box<Diagram>`——递归结构，但 S19 约束禁止嵌套 steps，所以 `Step.diagram.steps` 必须为 None/空
- 共享外层 `diagram_type` 与 `config`——避免每个 step 重复声明，解析时将外层 config 合并到每个 step 的 diagram

### 6.4 ExportScene 扩展

在 [export-scene-spec.md](../specs/export-scene-spec.md) 中新增元素 ID 字段（动画锚点）：

```json
{
  "nodes": [{
    "entity": { "id": "api", ... },
    "layout": { ... },
    "style": { ... },
    "anchor_id": "node-api"     // ← 新增：SVG 元素 id
  }],
  "edges": [{
    "index": 0,
    "relation": { ... },
    "layout": { ... },
    "style": { ... },
    "anchor_id": "edge-0"       // ← 新增：SVG 元素 id
  }]
}
```

**ID 生成规则**（确定性，遵守 [AGENTS.md §2](../../AGENTS.md)）：

| 元素 | ID 格式 | 来源 |
|------|---------|------|
| 节点 | `node-{entity.id}` | `ExportNode.entity.id` |
| 边 | `edge-{index}` | `ExportEdge.index` |
| 分组 | `group-{group.id}` | `ExportGroup.group.id` |
| 边标签 | `edge-{index}-label` | 派生 |

> **注意**：`entity.id` 符合 `[a-z][a-z0-9_]*`，可直接用于 SVG id（无需转义）。`index` 为 usize，转字符串即可。

### 6.5 渲染参数扩展

`RenderRequest` 新增字段：

```rust
pub struct RenderRequest {
    // ... 现有字段
    pub animation: Option<AnimationSpec>,
    pub edge_flow: bool,           // 自动循环动画开关
    pub compat_mode: CompatMode,   // 兼容模式，默认 Modern
}

pub struct AnimationSpec {
    pub kind: AnimationKind,
    pub duration_ms: u32,          // 默认 400
    pub easing: Easing,            // 默认 EaseInOut
    pub delay_ms: u32,             // 默认 0
}

pub enum AnimationKind {
    Transition { changes: ChangeSet },
    Directives(Vec<AnimationDirective>),
    Steps { frames: Vec<StepFrame> },
}

pub enum AnimationDirective {
    Highlight { target: Identifier },
    Activate { target: Identifier },
    Flow { edge_index: usize },
    Focus { target: Identifier },
}

pub enum Easing {
    Linear, EaseIn, EaseOut, EaseInOut,
    CubicBezier(f64, f64, f64, f64),
}

pub enum CompatMode {
    Modern,   // CSS @keyframes + transition（默认，含 transform-box:fill-box 缩放动画）
    Safe,     // 仅 CSS transition，不使用 scale 动画（兼容旧浏览器）
    Static,   // 无动画（纯静态 SVG）
}
```

### 6.6 明确不扩展的项

- ❌ DSL 中不引入 `animate` / `transition` / `keyframe` 关键字
- ❌ DSL 中不引入节点级动画属性（`A { animate: pulse }`）
- ❌ DSL 中不引入时间轴/关键帧语法
- ❌ DSL 中不引入 `step` 单数关键字（只用 `steps:` 复数块）

理由：动画参数属于渲染关注点，应由渲染层参数化。这与 [competitive-strategy.md §4.5](../product/competitive-strategy.md) "动画数据由渲染器自动生成，用户和 Agent 不需要控制动画参数"一致。

---

## 7. 项目实现路径

### 7.1 前置依赖

**必须先完成 Layout Intent MVP（P1a/P1b）**——[layout-refinement-todo.md](../architecture/intent/layout-refinement-todo.md) 显示当前 Grid Snap Phase 3 阻塞在 Layout Intent。动画工作应在 Layout Intent MVP 完成后启动，因为：
- Layout Intent 的位置微调是 Patch 动画的高价值场景
- 动画需要稳定的布局 seed，Layout Intent 的确定性输出是基础

### 7.2 四阶段实施计划

#### 阶段一：基础设施 + Patch 动画（最小可行）

**目标**：Agent 微调时能产出带过渡动画的 SVG。

**任务清单**：

| # | 任务 | 涉及文件 | 验收标准 |
|---|------|----------|----------|
| 1.1 | SVG 元素 ID 注入 | `render/paint/scene_svg.rs`、`render/paint/standard.rs`、`render/paint/sequence.rs`、`render/paint/er.rs`、`render/paint/node.rs`、`render/paint/edge.rs` | 每个 `<g>` 节点带 `id="node-{id}"`，每条边带 `id="edge-{index}"` |
| 1.2 | ExportScene 增加 `anchor_id` 字段 | `render/scene.rs`、[export-scene-spec.md](../specs/export-scene-spec.md) | 字段在 nodes/edges/groups 中出现，schema_version 升级 |
| 1.3 | 新增动画模块 | `render/paint/animation.rs`（新文件） | 实现 `encode_animation_style()` 输出 `<defs><style>` 包含 `@keyframes` + class 定义；实现 `encode_transition()` 为变更元素注入初始状态 class |
| 1.4 | ChangeSet → CSS class 映射 | `render/paint/animation.rs` | Add→`.dfy-enter`(opacity:0;scale:0→1)、Remove→`.dfy-exit`(opacity:1→0)、Modify位置→初始`transform:translate()` + `.dfy-move`(transition)、Modify属性→`transition:fill/stroke`、边路径变更→交叉淡入淡出（旧边`.dfy-fade-out`+新边`.dfy-fade-in`） |
| 1.5 | 扩展 RenderRequest | `render/request.rs` | 增加 `animation`、`edge_flow`、`compat_mode` 字段 |
| 1.6 | CLI 暴露过渡接口 | `drawify-cli` | `drawify render --transition old.dfy new.dfy --changes c.json -o out.svg` |
| 1.7 | Server 暴露过渡接口 | `drawify-server` | `POST /api/v1/render` 支持 `animation` 字段 |
| 1.8 | WASM 暴露过渡接口 | `drawify-wasm` | `renderWithTransition(oldSource, newSource, changes, opts)` |
| 1.9 | 兼容性降级 | `render/paint/animation.rs` | `CompatMode::Safe` 时不输出 scale 动画（仅 opacity+translate）；`Static` 时无 `<style>` 动画块 |
| 1.10 | 可访问性 | `render/paint/svg_utils.rs` | SVG `<style>` 内嵌 `@media (prefers-reduced-motion: reduce)` 全局禁用动画 |
| 1.11 | 确定性保证 | `render/paint/animation.rs` | 动画 class 注入按 ChangeSet 的 changes 顺序（Vec，非 HashMap）；@keyframes 定义顺序固定；不依赖排序不稳定的容器 |
| 1.12 | `transform-box` 处理 | `render/paint/animation.rs` | Modern 模式用 `transform-box:fill-box; transform-origin:center`；Safe 模式用像素坐标计算 transform-origin，避免不兼容 |

**验收 Demo**：

```bash
# 准备 v1.dfy（A -> B）和 v2.dfy（A -> B -> C），以及 changeset.json
drawify render --transition v1.dfy v2.dfy --changes changeset.json -o out.svg
# out.svg 双击打开，C 节点通过 CSS @keyframes 弹性缩放+淡入出现，A/B 位置通过 CSS transition 平滑过渡
```

#### 阶段二：参数化指令 + 循环动画

**目标**：支持 `highlight` / `activate` / `flow` / `focus` 指令，边线数据流循环动画。

**任务清单**：

| # | 任务 | 涉及文件 |
|---|------|----------|
| 2.1 | `AnimationDirective` 枚举与 CSS class 注入 | `render/paint/animation.rs` |
| 2.2 | 边线数据流 CSS keyframes | `render/paint/svg_utils.rs`（`<style>` 内嵌） |
| 2.3 | CLI 支持 `--animate` flag | `drawify-cli` |
| 2.4 | Server 支持 `animation.kind: Directives` | `drawify-server` |
| 2.5 | Studio 前端增加动画参数 UI | `studio/` |

**验收 Demo**：

```bash
drawify render arch.dfy --animate highlight:auth,flow:edge-2 -o out.svg
# out.svg 中 auth 节点高亮，edge-2 边线数据流动
```

#### 阶段三：Steps 系统 + HTML 导出

**目标**：DSL 声明 `steps:` 块，渲染为可播放的 HTML。

**任务清单**：

| # | 任务 | 涉及文件 |
|---|------|----------|
| 3.1 | AST 扩展 `Diagram.steps` + `Step` 结构 | `ast.rs`、[ast-spec.md](../specs/ast-spec.md) |
| 3.2 | Lexer 识别 `steps` / `as` 关键字 | `lexer.rs` |
| 3.3 | Parser 解析 `steps:` 块 | `parser.rs` |
| 3.4 | Validator 校验 S17-S21 约束 | `validation/` |
| 3.5 | diff2 支持 Step 间 diff | `diff2/diff.rs`（对每个相邻 step 对调用 `diff`） |
| 3.6 | Steps 渲染管线（多帧 + 帧间 CSS 过渡） | `render/paint/animation.rs` | 每个 Step 渲染为完整 SVG 帧，帧间通过 CSS class 切换触发 `@keyframes`/`transition` 过渡动画 |
| 3.7 | HTML 自包含导出编码器 | `render/encode/html_animation.rs`（新文件） | 输出自包含 HTML，内嵌 SVG 帧 + CSS 动画样式 + JS 播放器 |
| 3.8 | 纯 SVG Steps 导出（无 JS） | `render/encode/mod.rs` | 可选导出纯 SVG 版 Steps，用 CSS `:target` 伪类或 SVG 内 `<rect>` 按钮做最简帧切换，可嵌入 GitHub/Notion |
| 3.9 | 轻量 JS 播放器（< 2KB） | 内嵌于 HTML 导出 | JS 仅做 class 切换触发 CSS 过渡，不做属性插值；支持上一步/下一步/自动播放/跳转 |
| 3.10 | CLI 支持 `--format html-animation` | `drawify-cli` |
| 3.11 | Server 支持 `POST /api/v1/render/steps` | `drawify-server` |
| 3.12 | WASM 支持 `renderSteps` | `drawify-wasm` |
| 3.13 | DSL 语法文档更新 | [language-spec.md](../specs/language-spec.md)、[dsl-writing-manual.md](../specs/dsl-writing-manual.md) |

**验收 Demo**：

```bash
drawify render auth-flow.dfy --format html-animation -o demo.html
# demo.html 双击打开，浏览器中可点击"下一步/上一步/自动播放"
```

#### 阶段四：交互增强 + 光栅化导出（P2/P3）

**目标**：导出 SVG 自带 hover 交互；GIF/MP4 导出支持 PPT 嵌入。

**任务清单**：

| # | 任务 | 优先级 |
|---|------|--------|
| 4.1 | CSS `:hover` 交互动画（导出 SVG 自带） | P2 |
| 4.2 | Studio JS 事件增强（关联高亮、多选） | P2 |
| 4.3 | Playwright GIF/MP4 导出 | P3 |
| 4.4 | SVG 序列导出（每步一个 SVG） | P3 |

### 7.3 实施顺序与依赖

```
阶段一（基础设施 + Patch 动画）
   │
   ├── 1.1 SVG 元素 ID 注入 ← 所有动画的前置条件
   ├── 1.2 ExportScene anchor_id
   ├── 1.3-1.4 动画模块 + ChangeSet 映射
   ├── 1.5 RenderRequest 扩展
   ├── 1.6-1.8 CLI/Server/WASM 接口
   └── 1.9-1.11 降级/可访问性/确定性
        │
        ▼
阶段二（参数化指令 + 循环动画）  ← 可与阶段三并行
        │
        ▼
阶段三（Steps 系统 + HTML 导出）
   │
   ├── 3.1-3.4 AST/Lexer/Parser/Validator
   ├── 3.5 diff2 Step 间 diff
   ├── 3.6-3.8 渲染管线 + HTML 编码器 + JS 播放器
   └── 3.9-3.12 接口暴露 + 文档
        │
        ▼
阶段四（交互增强 + 光栅化）  ← P2/P3，按需投入
```

### 7.4 性能基准目标

| 指标 | 目标 | 测量方法 |
|------|------|----------|
| Patch 动画首帧渲染 | < 100ms（100 节点图） | `performance.now()` |
| Steps 帧间过渡 | < 300ms（50 节点图） | 同上 |
| SVG 动画文件体积 | < 500KB（20 步 Steps） | 文件大小 |
| HTML 导出体积 | < 1MB（20 步 Steps） | 文件大小 |
| Studio 动画帧率 | ≥ 30fps | Chrome DevTools Performance |

---

## 8. 落地清单（Concrete Deliverables）

### 8.1 阶段一交付物（最小可行）

- [ ] `crates/drawify-core/src/render/paint/animation.rs`（新文件）
- [ ] `crates/drawify-core/src/render/paint/scene_svg.rs` 修改：注入元素 ID
- [ ] `crates/drawify-core/src/render/paint/{standard,sequence,er}.rs` 修改：paint 函数注入 id
- [ ] `crates/drawify-core/src/render/scene.rs` 修改：ExportNode/ExportEdge/ExportGroup 增加 `anchor_id`
- [ ] `crates/drawify-core/src/render/request.rs` 修改：增加 `animation` / `edge_flow` / `compat_mode`
- [ ] `crates/drawify-cli` 修改：`--transition` / `--animate` flag
- [ ] `crates/drawify-server` 修改：`/api/v1/render` 支持 `animation` 字段
- [ ] `crates/drawify-wasm` 修改：`renderWithTransition` 函数
- [ ] `docs/specs/export-scene-spec.md` 更新：`anchor_id` 字段、schema_version 升级
- [ ] `benchmarks/` 新增：动画性能基准测试
- [ ] 单元测试：ChangeSet → CSS class 映射的每个分支（Add/Remove/Modify 对应正确 class）
- [ ] 集成测试：`render --transition` 端到端，验证 `<style>` 块正确输出、class 正确注入

### 8.2 阶段三交付物（Steps 系统）

- [ ] `crates/drawify-core/src/ast.rs` 修改：`Diagram.steps` + `Step` 结构
- [ ] `crates/drawify-core/src/lexer.rs` 修改：`steps` / `as` 关键字
- [ ] `crates/drawify-core/src/parser.rs` 修改：`steps:` 块解析
- [ ] `crates/drawify-core/src/validation/` 修改：S17-S21 约束
- [ ] `crates/drawify-core/src/diff2/diff.rs` 修改：Step 间 diff
- [ ] `crates/drawify-core/src/render/encode/html_animation.rs`（新文件）
- [ ] `crates/drawify-core/src/render/encode/mod.rs` 修改：注册 `HtmlAnimation` 格式
- [ ] `docs/specs/language-spec.md` 更新：`steps:` 语法、保留字、约束
- [ ] `docs/specs/ast-spec.md` 更新：`Diagram.steps` + `Step` 结构
- [ ] `docs/specs/dsl-writing-manual.md` 更新：Steps 用法示例
- [ ] showcase 示例：Steps 演示案例

---

## 9. 与现有文档的关系

| 现有文档 | 关系 |
|----------|------|
| [svg-embedding-design-impact.md](./svg-embedding-design-impact.md) | **底层约束**：嵌入方式能力边界决定了导出格式分层、CSS 放置、ID 策略等核心决策 |
| [animation-capability-research.md](./animation-capability-research.md) | 本方案是其落地版，将研究结论转为可执行任务 |
| [export-format-guide.md](./export-format-guide.md) | 用户视角的导出格式选型（基于本方案的能力矩阵） |
| [competitive-strategy.md](../product/competitive-strategy.md) §4.5 | 本方案细化语义动画的 P1/P2 优先级与具体产品形态 |
| [language-spec.md](../specs/language-spec.md) | 本方案新增 `steps:` 语法、保留字、约束 S17-S21 |
| [ast-spec.md](../specs/ast-spec.md) | 本方案新增 `Diagram.steps` + `Step` 结构 |
| [export-scene-spec.md](../specs/export-scene-spec.md) | 本方案新增 `anchor_id` 字段 |
| [layout-refinement-todo.md](../architecture/intent/layout-refinement-todo.md) | 动画工作在 Layout Intent MVP 完成后启动 |
| [cytoscape-js-research.md](../architecture/参考资料/cytoscape-js-research.md) | "静态导出，非交互探索"——本方案的交互动画限定为导出 SVG 自带的轻交互 |
| [diff2/README.md](../../crates/drawify-core/src/diff2/README.md) | `ChangeSet` 是 Patch 动画的语义源 |
| [AGENTS.md](../../AGENTS.md) | 遵守 §1 无向后兼容（直接改 AST/DSL）、§2 确定性迭代（动画编排用 Vec 不用 HashMap） |

---

## 10. 下一步行动

1. **等待 Layout Intent MVP 完成**（P1a/P1b），动画工作随后启动
2. **启动阶段一**：SVG 元素 ID 注入 + Patch 动画模块（最小可行）
3. **同步更新** [language-spec.md](../specs/language-spec.md) 与 [ast-spec.md](../specs/ast-spec.md)（阶段三启动时）
4. **同步更新** [export-scene-spec.md](../specs/export-scene-spec.md) `anchor_id` 字段（阶段一启动时）
5. **建立性能基准**：`benchmarks/` 下新增动画性能测试

---

## 修订记录

| 版本 | 日期 | 说明 |
|------|------|------|
| 0.2.0 | 2026-06-24 | 技术路线重大调整：全面放弃 SMIL，采用 SVG 内嵌 CSS（`@keyframes` + `transition`）方案。更新 §0/§1.3/§3.2/§3.3/§4.1/§4.2/§4.3/§7 等章节，任务清单改为 CSS class 注入方案，新增纯 SVG Steps 导出选项、`transform-box` 处理任务。 |
| 0.1.0-draft | 2026-06-23 | 初稿：基于研究文档产出可落地方案，覆盖产品设计/用户使用/实现路径/DSL 扩展 |
