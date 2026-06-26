# 动画导出格式选型指南

> 版本：0.1.0-draft | 状态：产品文档 | 日期：2026-06-24
> 关联：[svg-embed-comparison.html](./svg-embed-comparison.html)（嵌入方式实测）、[animation-capability-research.md](./animation-capability-research.md)、[animation-implementation-plan.md](./animation-implementation-plan.md)

本文面向产品文档与 Playground 导出提示文案，按三类用户给出**推荐导出格式**与**用户可预期的效果**。

---

## 0. 前置知识：动画能不能播，取决于两件事

### 0.1 SVG 内部的 CSS 动画 vs 外部 JS 控制

实测见 [svg-embed-comparison.html](./svg-embed-comparison.html)：CSS 写在 SVG 文件内部的 `<defs><style>` 里时，以下嵌入方式**内部 CSS 动画均可播放**：

| 嵌入方式 | 内部 CSS 动画 | 外部 JS 控制 | `:hover` 等交互 |
|----------|---------------|--------------|-----------------|
| 直接打开 `.svg` | ✅ | — | ✅ |
| `<img src="*.svg">` | ✅ | ❌ | ❌ |
| `<object data="*.svg">` | ✅ | ⚠️ 同域可 | ✅ |
| `<iframe src="*.svg">` | ✅ | ⚠️ 同域可 | ✅ |
| 内联 `<svg>`（HTML 里） | ✅ | ✅ | ✅ |

**结论**：不必把 CSS 写在 HTML 里；CSS 内嵌于 SVG 即可。`<img>` 不能播动画是常见误解——它不能的是**外部 JS 改 class 触发的分步播放**，不是 `@keyframes` 自播放。

### 0.2 平台净化 vs 浏览器嵌入

即使浏览器里 `<img>` 能播 SVG 内 CSS 动画，**目标平台仍可能净化 SVG**（去掉 `<style>`、`<script>`、事件属性），或把图栅格化成静态位图。选型时要同时看「浏览器能力」和「投递渠道」。

| 限制类型 | 典型表现 |
|----------|----------|
| 浏览器嵌入隔离 | `<img>` 下无 hover、无外部 JS 切 Step |
| 平台 SVG 净化 | README 托管方剥离 `<style>` → 退化为静态 |
| 非 Web 渠道 | PPT / PDF / 邮件 / IM 贴图 → 通常只显示首帧或静态图 |
| 跨域与安全头 | HTML iframe 需 `frame-ancestors`、父页 CSP 放行 |

---

## 1. 开发者

**画像**：通过 CLI / API / MCP / CI 集成 Drawify；图出现在 GitHub、文档站、内部 Dashboard；关注可脚本化、可版本管理、Patch 变更可感知。

### 1.1 推荐格式对照表

| 场景 | 推荐格式 | 用户看到的预期效果 | 注意 |
|------|----------|-------------------|------|
| README / PR 评论插图 | **SVG**（`compat_mode: modern`） | 托管 URL + `![](url)` 嵌入时，**入场、边绘制、数据流等自播放 CSS 动画可见**（现代浏览器） | 依赖托管不净化 `<style>`；无 hover、无外部切 Step |
| CI 产物 / 静态站点 | **SVG** 或 **PNG** | SVG：矢量 + 可选自播放动画；PNG：纯静态、兼容性最广 | 静态归档选 PNG；要动画选 SVG |
| Agent Patch 后「改了什么」 | **SVG**（Patch 过渡） | 打开或嵌入后看到节点平移、增删淡入淡出 | 单次自动过渡，无需用户操作 |
| 本地预览 / 调试 | **SVG** 直接打开 | 双击即有动画；`:hover` 可用 | 与 `<img>` 嵌入行为不同 |
| 嵌入自有 Web 应用 | **内联 SVG** 或 **HTML 动画** | 前端可 `getElementById` 控制；Steps 可编程 | Studio / WASM 场景 |
| API 返回给下游渲染 | **JSON**（ExportScene）+ 可选 **SVG** | 下游自定呈现；SVG 即开即用 | JSON 无动画，需自绘 |
| 需要逐步讲解（发布会） | **HTML 动画** | 浏览器打开：上一步 / 下一步 / 自动播放 | 非 PPT 场景首选 |

### 1.2 Playground / CLI 提示文案（开发者）

```
推荐：导出 SVG
• 适合：GitHub、文档站、CI 附件
• 效果：CSS 动画内嵌在 SVG 内，用 <img> 或链接嵌入即可自播放（入场、数据流等）
• 限制：无法由页面 JS 切换步骤；无 hover（<img> 嵌入时）

需要分步播放？→ 导出 HTML 动画
需要绝对静态？→ 导出 PNG，或 SVG + compat_mode: static
```

---

## 2. 技术写作者

**画像**：写技术博客、产品文档、Notion / Confluence / 语雀；关心「一张图讲清流程」、维护成本低、链接稳定。

### 2.1 推荐格式对照表

| 场景 | 推荐格式 | 用户看到的预期效果 | 注意 |
|------|----------|-------------------|------|
| 博客 / 静态文档站（自建） | **SVG** 或 **HTML 动画** | SVG：插图自播放轻量动画；HTML：完整 Steps + 播放条 | 自建站可用 `<object>` 获得 hover |
| GitHub / GitLab 文档 | **SVG**（CDN 托管） | Markdown 图片链接；**内部 CSS 动画可播** | 确认平台未剥离 `<style>` |
| Notion / 飞书文档 | **PNG** 或 **GIF**（Steps） | 上传后稳定显示；动画用 GIF | 多数块编辑器对 SVG 动画支持弱 |
| 「先 A 后 B 再 C」单图叙事 | **HTML 动画**（Steps） | 读者浏览器打开：分步展开 + 过渡 | 文档内链「打开演示」而非内嵌 |
| 打印 / PDF 导出 | **PNG** 或 **SVG**（`static`） | 固定版面、无动画干扰 | 动画在 PDF 中一般不播放 |
| 架构概览（无需逐步） | **SVG** | 边数据流循环表达调用方向 | `edge_flow` 可关 |

### 2.2 Playground 提示文案（技术写作者）

```
写 README？→ SVG
• 托管后 ![](/path.svg) 即可，入场和数据流动画会自动播放

写教程「分几步讲」？→ HTML 动画
• 给读者一个链接，浏览器里播放，比贴多张 PNG 好维护

发到 Notion / 飞书？→ PNG（静态）或 GIF（要动效时）
• 块编辑器通常不把 SVG 当动画载体
```

---

## 3. 企业培训

**画像**：架构评审、变更宣讲、新人培训、合规材料；常用 PPT、Teams/Zoom 共享、LMS；要「讲得清楚」且「能存档」。

### 3.1 推荐格式对照表

| 场景 | 推荐格式 | 用户看到的预期效果 | 注意 |
|------|----------|-------------------|------|
| 会议室 live 讲解 | **HTML 动画**（全屏浏览器） | Steps + 自动播放 + 上/下一步；Patch 过渡顺滑 | 共享屏幕打开链接，不依赖 PPT |
| 架构变更评审 | **HTML 动画** 或 **SVG**（Patch） | 变更前后对比有过渡，观众一眼看出节点移动 | SVG 适合录屏后放进纪要 |
| 嵌入企业内网 Wiki | **HTML**（iframe）或 **SVG** | iframe：完整播放器；SVG：轻量自播放 | 需 IT 放行 `frame-ancestors` / CSP |
| 塞进 PPT / Keynote | **PNG**（静帧）或 **GIF/MP4**（P2） | 每 Step 一帧或一段录像 | **PPT 不播放 SVG/HTML 动画** |
| 培训 LMS 离线包 | **HTML**（自包含） | 双击离线打开，无 CDN 依赖 | 单文件或 zip |
| 合规归档 / 审计留痕 | **PNG** + **JSON**（ChangeSet） | 静态快照 + 机器可读变更记录 | 动画不作为法律依据 |
| 邮件分发讲义 | **PNG** | 附件即见 | 不用 SVG/HTML |

### 3.2 Playground 提示文案（企业培训）

```
现场讲架构演变？→ HTML 动画
• 浏览器全屏演示，支持分步与自动播放

要放进 PPT？→ 每步导出 PNG，或等待 GIF/MP4 导出（规划中）
• PowerPoint 不会播放 SVG 内动画

内网 Wiki 嵌入？→ HTML（iframe）或 SVG
• HTML：完整控制条；SVG：轻量、自播放，但不能在页面上点「下一步」
```

---

## 4. 格式能力总览（三类用户共用）

| 格式 | 矢量 | 自播放 CSS 动画 | 分步 Steps | hover 交互 | GitHub `![](url)` | PPT | 离线 |
|------|------|-----------------|------------|------------|-------------------|-----|------|
| **SVG** | ✅ | ✅（内嵌 CSS） | ❌ 需 JS | ⚠️ 非 `<img>` 时 | ✅ 动画可播* | 静帧 | ✅ |
| **HTML 动画** | ✅ | ✅ | ✅ | ✅ | ❌ 需链接 | ❌ | ✅ |
| **PNG** | ❌ | ❌ | ❌ | ❌ | ✅ | ✅ | ✅ |
| **GIF / MP4**（P2） | ❌ | ✅ 光栅 | ✅ | ❌ | ⚠️ | ✅ | ✅ |
| **JSON** | — | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ |

\* 前提：托管方不净化 SVG 的 `<style>`；嵌入方式为图片标签时无 hover、无 JS 切 Step。

---

## 5. 决策简图

```
需要动画吗？
├─ 否 → PNG（最广）或 SVG static（矢量）
└─ 是
   ├─ 只需自动播（入场 / 数据流 / Patch 一次过渡）→ SVG
   ├─ 要分步讲解 + 播放控制 → HTML 动画
   ├─ 要嵌在自有页面里用 JS 控制 → 内联 SVG 或 HTML
   └─ 要进 PPT / 邮件 / 纯静态 PDF → PNG 或 GIF/MP4（P2）
```

---

## 6. 与实现路线的对应

| 导出格式 | 实现状态 | 代码入口（规划/现有） |
|----------|----------|----------------------|
| SVG + 内嵌 CSS | 规划中 | `render/paint/animation.rs`、`scene_svg` |
| HTML 动画 | 规划中 | `render/encode/html_animation.rs` |
| `compat_mode: static` | 规划中 | `RenderRequest.compat_mode` |
| GIF / MP4 | P2 | Playwright + ffmpeg |

---

## 7. 相关文档

| 文档 | 内容 |
|------|------|
| [svg-embedding-design-impact.md](./svg-embedding-design-impact.md) | SVG 嵌入方式对 Drawify 设计的底层约束（必读） |
| [animation-capability-research.md](./animation-capability-research.md) | 动画能力竞品调研与技术选型 |
| [animation-implementation-plan.md](./animation-implementation-plan.md) | 分阶段落地计划 |
| [svg-embed-comparison.html](./svg-embed-comparison.html) | 四种嵌入方式实测对比 |
| [loop-img-test.html](./loop-img-test.html) | `<img>` 中 CSS 循环动画验证 |
| [btn-in-img-test.html](./btn-in-img-test.html) | `<img>` 中 SVG 按钮/交互失效验证 |
| [button-svg-collab.html](./button-svg-collab.html) | SVG / HTML 协作模式演示 |
| [standalone-animated.svg](./standalone-animated.svg) | 实验用自包含动画 SVG |
