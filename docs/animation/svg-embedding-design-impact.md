# SVG 嵌入方式对 Drawify 动画设计的影响

> 版本：1.0.0 | 状态：设计文档 | 日期：2026-06-24
> 关联：[animation-capability-research.md](./animation-capability-research.md)、[animation-implementation-plan.md](./animation-implementation-plan.md)、[export-format-guide.md](./export-format-guide.md)
> 实测验证：[svg-embed-comparison.html](./svg-embed-comparison.html)、[loop-img-test.html](./loop-img-test.html)、[btn-in-img-test.html](./btn-in-img-test.html)

---

## 0. 摘要

本文档是对 Drawify 动画能力设计决策的底层依据。我们在实现动画之前，必须回答一个问题：**同一份 SVG 文件，在不同嵌入方式下，用户能看到什么、能做什么？**

经过实测验证，我们得出如下核心结论，这些结论直接约束了后续所有动画功能的设计：

1. **CSS 必须写在 SVG 内部的 `<defs><style>` 里**（不是 HTML 的 `<style>`），这样所有嵌入方式下自播放动画都能生效。
2. **`<img>` 标签是 SVG 嵌入最普遍的方式（GitHub/Notion/文档站），但它完全隔离交互**：不响应点击/hover、不执行内部 JS、不响应外部 JS。这意味着自动播放的动画可以在 `<img>` 中工作，但交互和分步控制不行。
3. **因此必须严格区分两种导出格式**：纯 `.svg`（自播放动画、无交互按钮）和 `.html`（完整播放器 UI、内联 SVG + JS 控制）。
4. **SMIL 作为交互控制手段在 `<img>` 下完全失效**，且其路径变形（`d` 属性插值）存在点数对齐难题，因此全面放弃 SMIL，统一使用 CSS `@keyframes` + CSS `transition`。
5. **所有 SVG 元素必须有跨版本稳定 ID**——`node-{entity.id}` 对于节点足够，但边的 `edge-{index}` 在增删后会错位，需要永久身份标识。

---

## 1. 五种 SVG 嵌入方式的浏览器级能力矩阵

这是核心事实表。所有能力在 Chrome/Firefox/Safari/Edge 最新版中均经过实测验证。

### 1.1 能力定义

| 能力项 | 含义 |
|--------|------|
| 自播放 CSS 动画 | SVG `<style>` 内 `@keyframes` + `animation` 属性，打开即播放，无需用户操作 |
| CSS `:hover`/`:active` | 鼠标悬停/点击触发 CSS 状态变化 |
| SMIL 按钮控制 | `<animate begin="btn.click">`——点击 SVG 内按钮触发动画 |
| SVG 内 `<a>` 链接 | `<a xlink:href="...">` 可点击跳转 |
| SVG 内 `<script>` | SVG 内部的 JS 代码能否执行 |
| 外部 JS 操作 SVG DOM | 父页面 JS 能否通过 `getElementById` 等方式修改 SVG 内部元素 |
| 外部 CSS 影响 SVG | 父页面 CSS 能否选择并样式化 SVG 内部元素 |

### 1.2 五种嵌入方式能力矩阵

| 能力项 | 直接打开 `.svg` | `<img src>` | `<object data>` | `<iframe src>` | 内联 `<svg>` |
|--------|:---:|:---:|:---:|:---:|:---:|
| 自播放 CSS 动画 | ✅ | ✅ | ✅ | ✅ | ✅ |
| CSS `:hover` 伪类 | ✅ | ❌ | ✅ | ✅ | ✅ |
| SMIL `begin="click"` 按钮 | ✅ | ❌ | ✅ | ✅ | ✅ |
| SVG 内 `<a>` 链接可点击 | ✅ | ❌ | ✅ | ✅ | ✅ |
| SVG 内 `<script>` 执行 | ✅ | ❌ | ✅ | ✅ | ✅ |
| 外部 JS 操控 SVG DOM | — | ❌ | ⚠️ 同域可 | ⚠️ 同域可 | ✅ |
| 外部 CSS 影响 SVG | — | ❌ | ❌ | ❌ | ✅ |

### 1.3 关键观察

1. **自播放 CSS 动画是唯一"所有嵌入方式都支持"的能力**。这是唯一真正通用的动画能力，是纯 SVG 导出格式的全部基础。
2. **`<img>` 是唯一"隔离级别最高"的方式**——它把 SVG 当作静态光栅图像处理，任何交互、脚本、外部控制都被禁止。这是浏览器安全设计（防止 `<img src="evil.svg">` 执行恶意代码），无法绕过。
3. **`<object>` 和 `<iframe>` 的交互能力与直接打开一致**（同域下外部 JS 也可访问），但注意：这两种标签在**多数内容平台（GitHub/Notion 等）会被过滤**，所以在实际分发场景中用户几乎用不到。
4. **内联 SVG（HTML 中直接写 `<svg>...</svg>`）能力最强**——外部 JS/CSS 完全控制，适合自有 Web 应用和 HTML 导出格式。

---

## 2. 内容平台的过滤规则（比浏览器限制更真实）

浏览器支持是一回事，用户把图放到目标平台后能不能看到是另一回事。这是导出格式选型真正要考虑的边界条件。

| 平台/场景 | `<img>` | `<object>` | `<iframe>` | 内联 `<svg>` | 说 明 |
|-----------|:---:|:---:|:---:|:---:|------|
| GitHub README/Issues | ✅ | ❌ 过滤 | ❌ 过滤 | ❌ 过滤 | 只接受 `<img>`；且会经过 Camo 代理缓存 |
| GitLab Markdown | ✅ | ❌ | ❌ | ❌ | 同上 |
| Notion 文档 | ✅ | ❌ | ⚠️ 仅限 embed 块 | ❌ | embed 块只支持特定域名白名单 |
| Confluence | ✅ | ⚠️ 需管理员开启 | ⚠️ 需管理员开启 | ❌ | 默认只接受 `<img>` |
| 飞书/语雀文档 | ✅ | ❌ | ❌ | ❌ | 同上 |
| 自建技术博客（Hexo/VitePress 等） | ✅ | ✅ | ✅ | ✅ | 完全可控 |
| 本地 HTML 文件（双击打开） | ✅ | ✅ | ✅ | ✅ | `file://` 协议下有跨域限制 |
| 自建 Web 应用/Dashboard | ✅ | ✅ | ✅ | ✅ | 完全可控 |
| PPT/Keynote | ⚠️ 静帧 | ❌ | ❌ | ❌ | **PPT 不播放 SVG 动画，只显示首帧** |
| 邮件客户端（Outlook/Gmail） | ⚠️ 多数禁用 | ❌ | ❌ | ❌ | 多数邮件客户端禁用 SVG 或仅显示静帧 |
| Slack/Discord/IM | ✅（静帧） | ❌ | ❌ | ❌ | 多数 IM 会把 SVG 栅格化或不显示 |
| VS Code Markdown 预览 | ✅ | ❌ | ❌ | ⚠️ | 安全策略限制 |

### 2.1 真正重要的分发渠道

从上述平台分析可以得出：

- **开发者最常用的分发渠道是 GitHub/GitLab**，唯一可用嵌入方式是 `<img>`
- **`<img>` 下可用的唯一动画能力是"自播放 CSS 动画"**（打开即播，无需交互）
- **Steps 分步播放器不可能在 `<img>` 下工作**，必须是 HTML 自包含文件，用户通过链接打开

这不是浏览器兼容性问题，是**平台内容安全策略（CSP）和 Markdown 白名单过滤**决定的，无法通过技术手段绕过。

---

## 3. 对动画技术选型的设计影响

### 3.1 为什么全面放弃 SMIL

原方案中 SMIL 被考虑用于 Patch 动画和纯 SVG Steps 按钮。现在确认：

| SMIL 场景 | 问题 | 结论 |
|-----------|------|------|
| `<animate attributeName="opacity">` 做淡入淡出 | CSS `@keyframes` 同样能做，且兼容性更好、代码更清晰 | CSS 替代 |
| `<animate attributeName="d">` 做路径变形 | **点数对齐问题是硬伤**——不同拓扑的路径贝塞尔控制点数量不同，SMIL `d` 插值要求点数一致，否则直接跳变。需要复杂的对齐预处理 | 改用"旧路径淡出 + 新路径淡入"（交叉淡入淡出），CSS 即可 |
| `<animateMotion>` 沿路径运动 | CSS `offset-path` 可以替代，但考虑到 Drawify 不需要粒子沿路径运动（数据流用 `stroke-dashoffset` 即可），无需引入 | 不用 |
| `<animate begin="btn.click">` SVG 内按钮控制 | **在 `<img>` 下完全失效**——点击事件不穿透 | 不用于核心交互；仅作为"直接打开 .svg 时的附加能力" |

**SMIL 唯一不可替代的优势是 SVG 内声明式按钮交互**，但这种交互在最核心的分发渠道（GitHub `<img>`）下失效，所以没有理由把它作为主力技术。

**CSS 全面替代方案**：
- 淡入/缩放/平移/颜色变化 → CSS `@keyframes`
- 路径变更 → 交叉淡入淡出（旧路径 `opacity: 1→0`，新路径 `opacity: 0→1`）
- 边数据流 → `stroke-dasharray` + `stroke-dashoffset` 循环
- 入场动画 → `@keyframes` + `animation-fill-mode: forwards`
- Steps 帧切换 → JS class 切换（HTML 导出）或 CSS `:target`（纯 SVG 可选）

### 3.2 CSS 样式放置位置

**强制要求**：所有动画 CSS 必须写在 SVG 内部的 `<defs><style><![CDATA[...]]></style></defs>` 中。

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="...">
  <defs>
    <style><![CDATA[
      @keyframes dfy-enter {
        from { opacity: 0; transform: scale(0.5); }
        to   { opacity: 1; transform: scale(1); }
      }
      .dfy-enter {
        transform-origin: center;
        transform-box: fill-box;
        animation: dfy-enter 400ms cubic-bezier(0.34, 1.56, 0.64, 1) forwards;
      }
      @keyframes dfy-flow {
        to { stroke-dashoffset: -20; }
      }
      .dfy-flow {
        stroke-dasharray: 6 4;
        animation: dfy-flow 1s linear infinite;
      }
      @media (prefers-reduced-motion: reduce) {
        * { animation: none !important; }
      }
    ]]></style>
  </defs>
  <!-- 图形内容，元素加 class 触发动画 -->
  <g id="node-api" class="dfy-enter">...</g>
  <path id="edge-0" class="dfy-flow" d="..."/>
</svg>
```

注意事项：
- 用 `<![CDATA[...]]>` 包裹 CSS，防止 `{}` `<>` 等字符被 XML 解析器误读
- `transform-box: fill-box` + `transform-origin: center` 是 SVG 中元素以自身中心缩放/旋转的必要设置（否则 transform 以 SVG viewBox 原点为参照）
- 必须包含 `@media (prefers-reduced-motion: reduce)` 尊重用户无障碍偏好
- **HTML 导出格式中**，SVG 内联后，SVG 自己的 `<style>` 仍然生效；播放器的额外 CSS（按钮、播放条 UI）写在 HTML 的 `<style>` 里，不放入 SVG

---

## 4. 对导出格式分层的设计影响

基于以上能力边界，Drawify 必须提供两种导出格式，且它们之间有严格的职责划分：

### 4.1 SVG 导出（自播放型）

**目标场景**：GitHub README、文档站插图、CI 产物、Agent Patch 预览、本地预览

**能做什么**：
- ✅ Patch 过渡动画（节点增删淡入淡出、位置平移、边淡入淡出）
- ✅ 参数化指令动画（节点 `activate` 脉冲、边 `highlight` 高亮、边 `flow` 数据流循环）
- ✅ 入场/出场一次性动画
- ✅ 矢量缩放无损

**不能做什么**：
- ❌ Steps 分步交互按钮（`<img>` 下按钮不工作）
- ❌ 播放/暂停/上一步/下一步控制条 UI
- ❌ 时间轴/进度条

**CSS 放置**：全部在 SVG `<defs><style>` 内，自包含。

**嵌入方式**：用户用 `<img src="diagram.svg">` 贴到 Markdown 里即可，动画自动播放。

**SVG 内可以放按钮作为附加能力**（直接双击打开或 `<object>` 嵌入时可用），但这是"bonus"，不承诺 GitHub 场景可用。

### 4.2 HTML 导出（Steps 交互型）

**目标场景**：架构评审演示、培训讲解、发布会、需要分步展开的教程

**能做什么**：
- ✅ SVG 导出的全部能力（因为 SVG 内联在 HTML 里，自包含 CSS 动画依然生效）
- ✅ Steps 分步播放控制（上一步/下一步/自动播放/跳转指定步）
- ✅ 播放控制条 UI（按钮、进度条、步号显示）
- ✅ 过渡动画：帧间元素增删/移动/样式变化的流畅过渡
- ✅ 键盘快捷键（←/→/Space）
- ✅ 全屏演示模式（可选）

**HTML 结构**：
```html
<!DOCTYPE html>
<html>
<head>
  <style>
    /* 播放器 UI 样式（按钮、进度条） */
  </style>
</head>
<body>
  <div class="dfy-player">
    <svg>
      <defs>
        <style>/* 动画 CSS（和纯 SVG 导出共用） */</style>
        <!-- 滤镜、marker 等 -->
      </defs>
      <!-- 第 1 帧图形（有稳定 id） -->
      <!-- 第 2 帧图形（通过 class 控制显隐） -->
      <!-- ... -->
    </svg>
    <div class="dfy-controls">
      <button class="dfy-prev">← 上一步</button>
      <button class="dfy-play">▶ 自动播放</button>
      <button class="dfy-next">下一步 →</button>
      <div class="dfy-progress">...</div>
    </div>
  </div>
  <script>
    // 播放器 JS（纯 class 切换，不做属性插值）
    // 约 100-200 行，<2KB gzipped
  </script>
</body>
</html>
```

**JS 播放器的职责边界**：
- 只做**状态管理和 class 切换**：当前是第几步 → 哪些元素应该显示/激活/隐藏 → 切换 class
- **不做属性值计算和插值**：所有动画效果由 CSS 处理，JS 不计算 `transform`/`opacity` 的中间值
- 通过元素 id 精确定位：`document.getElementById('node-api')` 然后 `.classList.add/remove/toggle`
- 体积控制在 2KB gzipped 以内

### 4.3 PNG / 静态 SVG 导出（无动画型）

**目标场景**：PPT、邮件、合规归档、需要绝对静态的场合

- SVG `compat_mode: static`：不输出任何 `<style>` 动画，图形直接以最终状态渲染
- PNG 导出：渲染后截图，纯静态

### 4.4 格式能力总览

| 能力 | SVG 自播放 | HTML Steps | PNG 静态 |
|------|:---:|:---:|:---:|
| 矢量无损 | ✅ | ✅ | ❌ |
| Patch 过渡动画 | ✅（一次） | ✅（多步） | ❌ |
| 数据流动画（循环） | ✅ | ✅ | ❌ |
| 节点激活脉冲 | ✅ | ✅ | ❌ |
| Steps 分步播放 | ❌ | ✅ | ❌ |
| 播放控制 UI | ❌ | ✅ | ❌ |
| GitHub `<img>` 嵌入可见 | ✅ | ❌（需链接打开） | ✅ |
| PPT/邮件可用 | ⚠️ 仅静帧 | ❌ | ✅ |
| 双击本地打开可用 | ✅ | ✅ | ✅ |
| 离线可用 | ✅ | ✅ | ✅ |
| 体积 | 小（10-50KB） | 中（30-100KB） | 中 |

---

## 5. 对元素 ID 策略的设计影响

### 5.1 为什么元素必须有稳定 ID

在 HTML Steps 导出格式中，JS 播放器通过 `getElementById` 定位元素来切换 class。如果元素 id 在不同帧之间不稳定（比如因为 index 变化导致同一实体的 id 变了），跨帧过渡动画就无法正确关联"旧图的这个元素"和"新图的这个元素"。

同样，Patch 动画需要 diff 旧场景和新场景：
- 哪些元素是新增的（加 `dfy-enter`）
- 哪些元素是删除的（加 `dfy-exit`，动画结束后移除）
- 哪些元素移动了（比较 transform 差异，触发 transition）
- 哪些元素属性变了（颜色、线宽等，触发 transition）

这一切的前提是：**同一实体在不同版本中 id 必须一致**。

### 5.2 节点 ID 策略

节点 id：`node-{entity.id}`

✅ 足够稳定。`entity.id` 是语义层分配的永久标识，不会因为其他节点增删而变化。

### 5.3 边 ID 策略（必须修正）

原方案：`edge-{index}`——按 `Vec<Relation>` 的位置索引编号

❌ **不稳定**。考虑以下场景：
- 版本 1 有边：`[A→B, A→C]` → edge-0=A→B, edge-1=A→C
- 在 A→B 前面插入 B→C，版本 2 边：`[B→C, A→B, A→C]` → edge-0=B→C, edge-1=A→B, edge-2=A→C
- diff 结果：edge-0（原 A→B）变成了 B→C → 误判为"A→B 被删除、B→C 新增"，而实际上 A→B 还在，只是位置变了
- 这会导致 Patch 动画错误：A→B 会被播放退出动画，而实际上它只是移动到了 edge-1

**正确方案**：边也需要永久身份标识。可选方案：

| 方案 | 说明 | 优缺点 |
|------|------|--------|
| **A：Relation 加永久 id** | 在语义层/AST 给每个 Relation 分配一个稳定 id（如 `rel_0`, `rel_1`... 或基于内容哈希），边 id 为 `edge-{relation.id}` | 最可靠，但需要修改 AST 结构 |
| **B：内容哈希** | 用 `hash(from_id, to_id, label)` 作为边 id，如 `edge-a-b-query` | 不需要改 AST，但同两点间多条同标签边会冲突（可用 label 区分或加序号） |
| **C：拓扑排序后编号** | 每次渲染前对边按稳定规则排序（from→to→label 字典序），再编号 | 简单，但同两点多条边顺序问题需要处理 |

**推荐方案 A**：和节点一致，在 AST 层给 Relation 分配永久 id。这是最根本的解决方案，也方便未来其他功能（如针对单条边的参数化指令）引用特定边。

### 5.4 子元素 ID 策略

组内子元素（文本、矩形、路径等）如果不需要被单独控制，不需要独立 id——动画加在组 `<g>` 上即可（如整个 `<g id="node-api" class="dfy-enter">` 统一做淡入缩放）。

需要单独动画的子元素（如节点内的 icon、边的 arrowhead）可以加 `id="{parent-id}-icon"`、`id="{edge-id}-arrow"` 等有规律的后缀。

---

## 6. 对动画能力分层的设计影响

基于"自播放动画是 SVG 唯一通用能力"这一事实，Drawify 的动画能力应分为三层：

### Layer 1：自播放动画（纯 SVG 即可，所有嵌入方式生效）

这些动画打开 SVG 就自动播放，无需任何交互，在 `<img>` 下也能正常工作：

| 动画类型 | 触发时机 | CSS 实现 |
|----------|----------|----------|
| 节点入场（新增） | Patch Add | `@keyframes dfy-enter`（fade + scale），自动播放一次 |
| 节点出场（删除） | Patch Remove | `@keyframes dfy-exit`（fade + shrink），播放完后通过 CSS `visibility: hidden` 隐藏（因为纯 SVG 无法移除 DOM） |
| 节点位置变化 | Patch Modify（位置变） | CSS `transition: transform 300ms` |
| 边入场/出场 | Patch Add/Remove | `opacity` transition（路径变形用交叉淡入淡出代替） |
| 边数据流动画 | 指令 `animate=flow` | `stroke-dashoffset` infinite 循环 |
| 节点激活脉冲 | 指令 `animate=activate` | `@keyframes` 发光/缩放脉冲 infinite |
| 边高亮流动 | 指令 `animate=highlight` | `@keyframes` 颜色/透明度脉冲 |

**约束**：所有 Layer 1 动画必须是"渲染时就确定了最终状态和动画参数"的，不依赖运行时计算，不依赖用户交互。

### Layer 2：交互动画（HTML 导出或内联 SVG 可用）

这些动画需要用户交互触发，只能在 HTML 导出或内联 SVG（自有 Web 应用）中使用：

| 动画类型 | 触发方式 | 实现 |
|----------|----------|------|
| 节点 hover 高亮 | 鼠标悬停 | CSS `:hover` |
| 节点 focus 聚焦（相关节点/边高亮，其他变暗） | JS 控制 | JS 加 `.dimmed` class，CSS transition |
| Steps 帧间过渡 | 点击播放按钮 | JS 切换帧 class，CSS transition 处理过渡 |
| 点击节点显示详情 | JS 控制 | JS 切换 class 显示/隐藏详情面板 |

**约束**：Layer 2 动画不写入纯 SVG 导出的 CSS 中，或者说即使写入了也只是在非 `<img>` 场景下作为附加体验。

### Layer 3：播放器 UI 动画（仅 HTML 导出）

播放器自身的 UI 动效：按钮 hover、进度条变化、步号切换等。这些是 HTML 部分的 CSS，不属于 SVG 的范畴。

---

## 7. 对 Remove 动画的特殊处理

在纯 SVG 自播放场景下，"删除"动画有一个特殊问题：**旧版本中被删除的元素，在新版本 SVG 中根本不存在，怎么播放退出动画？**

这个问题有两种处理方式：

### 方案 1：两个 SVG 叠加（推荐用于 HTML Steps）

HTML 导出格式中，播放器同时保留"旧帧"和"新帧"两个 SVG 层（叠放在同一位置），旧帧在上层，播放退出动画后隐藏旧帧、显示新帧。这是最可靠的方案，因为旧元素确实存在于 DOM 中。

### 方案 2：保留被删除元素 + `dfy-exit` class（用于 Patch 单次 SVG）

纯 SVG Patch 导出（单次过渡，不是 Steps）时，被删除的元素仍然保留在 SVG 中，但加上 `dfy-exit` class，CSS 让它播完退出动画后隐藏：

```css
.dfy-exit {
  animation: dfy-exit 400ms ease-in forwards;
}
@keyframes dfy-exit {
  from { opacity: 1; transform: scale(1); }
  to   { opacity: 0; transform: scale(0.5); visibility: hidden; }
}
```

注意：`forwards` + `visibility: hidden` 让元素最终不可见且不响应事件，但 DOM 节点还在。这对于"Patch 过渡一次性动画"场景是可以接受的——图已经更新完了，删掉的节点淡出后就不再可见，不影响视觉。

**限制**：如果一个节点被删除后又有同名新节点（id 相同），会冲突。但正常 Patch 流程中不会出现这种情况（节点不会刚被删又被加回来，加回来是新的 Patch 操作）。

---

## 8. 设计决策汇总

以下是基于本文档分析得出的、具有约束力的设计决策：

| # | 决策 | 依据 |
|---|------|------|
| D1 | CSS 是唯一动画技术，全面放弃 SMIL | §3.1：SMIL 路径变形有硬伤，按钮交互在 `<img>` 下失效，CSS 可覆盖所有场景 |
| D2 | 动画 CSS 全部内嵌在 SVG `<defs><style><![CDATA[...]]>` 中 | §3.2：保证所有嵌入方式下自播放动画生效 |
| D3 | 提供两种导出格式：纯 SVG（自播放）和 HTML（Steps 交互） | §4：`<img>` 隔离交互是平台限制，无法绕过 |
| D4 | 纯 SVG 导出中不放交互按钮（或仅作附加能力） | §4.1：按钮在 `<img>` 下不工作，承诺可用会导致用户困惑 |
| D5 | HTML Steps 播放器 JS 只做 class 切换，不做属性插值 | §4.2：保持 JS 精简（<2KB），动画全部交给 CSS |
| D6 | 边必须有永久稳定 ID，不能用 `edge-{index}` | §5.3：index 在增删后不稳定，导致 Patch 动画错位 |
| D7 | 必须包含 `prefers-reduced-motion` 媒体查询 | §3.2：无障碍要求 |
| D8 | SVG 内元素动画用 `transform-box: fill-box` | §3.2：否则 scale/rotate 以 viewBox 原点为参照 |
| D9 | 边路径变更用"旧路径淡出 + 新路径淡入"，不做 d 属性插值 | §3.1：点数对齐问题无法可靠解决 |
| D10 | Remove 动画：HTML 用双图层叠加，SVG Patch 用保留元素 + `dfy-exit` class | §7：DOM 中不存在的元素无法播放动画 |
| D11 | 纯 SVG 导出中使用 `CDATA` 包裹 CSS | §3.2：XML 解析安全 |

---

## 9. 验证清单

实现过程中及完成后，需要验证以下场景：

- [ ] 纯 SVG 用 `<img>` 嵌入 HTML：自播放动画正常（入场、数据流、脉冲）
- [ ] 纯 SVG 用 `<img>` 嵌入 HTML：hover/按钮无响应（符合预期，不是 bug）
- [ ] 纯 SVG 双击直接打开：动画正常，附加按钮（如有）可点击
- [ ] 纯 SVG 用 `<object>` 嵌入：动画正常，hover/按钮可用
- [ ] HTML Steps 导出双击打开：播放器 UI 正常，按钮点击切换帧，过渡动画流畅
- [ ] HTML Steps 导出离线（断网）打开：全部功能正常
- [ ] 手机浏览器打开 SVG/HTML：动画正常
- [ ] 系统开启"减少动态效果"（prefers-reduced-motion）：动画被禁用
- [ ] 边在中间插入/删除后，其他边的动画关联正确（验证稳定 ID）
- [ ] 多次连续 Patch（A→B→C→D）：每次过渡动画都正确
- [ ] PPT 插入 SVG：显示首帧（不会动，但不会变形/报错）
- [ ] 文件体积：纯 SVG < 50KB，HTML < 100KB（含 JS）

---

## 10. 相关文档与实测页面

| 文档/页面 | 内容 |
|-----------|------|
| [animation-capability-research.md](./animation-capability-research.md) | 动画能力竞品调研与技术选型（v0.2.0 已更新为 CSS 方案） |
| [animation-implementation-plan.md](./animation-implementation-plan.md) | 分阶段落地计划 |
| [export-format-guide.md](./export-format-guide.md) | 面向用户的导出格式选型指南 |
| [svg-embed-comparison.html](./svg-embed-comparison.html) | 四种嵌入方式动画效果实测 |
| [loop-img-test.html](./loop-img-test.html) | `<img>` 中 CSS 无限循环动画验证 |
| [btn-in-img-test.html](./btn-in-img-test.html) | `<img>` 中 SVG 按钮/链接/hover 失效验证 |
| [button-svg-collab.html](./button-svg-collab.html) | HTML 按钮与 SVG 协作模式演示 |
| [standalone-animated.svg](./standalone-animated.svg) | 自包含动画 SVG 样例（内嵌 CSS） |
| [btn-in-svg.svg](./btn-in-svg.svg) | SVG 内 SMIL 按钮样例 |
