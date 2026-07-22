# Editor 改造方案（原 Playground）

> 状态：P0 已实施（2026-07-22）  
> 日期：2026-07-22  
> 背景：原 Playground 与 Showcase、Studio 职责重叠，产品边界不清；对外更名为 **Editor**，重新定位为「零注册出图调校台」，并与未来 SaaS 版 Studio 分工。

相关文档：

- [competitive-strategy.md](../product/competitive-strategy.md) — Studio 场景下的「好看」及格线
- [success-roadmap.md](../product/success-roadmap.md) — Editor 30 秒 Wow、笔触切换
- [studio/README.md](../../studio/README.md) — Studio（Agent 绘图 SaaS）边界
- [showcase/README.md](../../showcase/README.md) — 示例画廊与角色前缀
- [playground/README.md](../../playground/README.md) — 当前代码实现（目录名暂保留）

---

## 0. 命名规范

### 0.1 决策摘要

| 层级 | 名称 | 说明 |
|------|------|------|
| **对外产品名** | **Editor**（Plotgram Editor） | 用户可见：标题、导航、按钮、文档 |
| **URL 主路径** | `/editor` | 生产与开发环境 canonical 路径 |
| **兼容重定向** | `/playground` → `/editor` | 301 或 SPA 内重定向，保留旧链接 |
| **代码目录** | `playground/` | **暂不 rename**，降低改动面；P2 可评估 `editor/` |
| **禁止** | Studio 指代本工具 | Studio 专指 SaaS（Agent + 云存） |

### 0.2 文案约定

- 导航 / 按钮：**Editor** 或 **在 Editor 中打开**；避免单独写「编辑器」（太泛）
- 副标题（可选）：**图表调校** / **出图编辑器**
- 文档与 PR：统一写 **Editor**；提及历史实现时可注「原 Playground」
- localStorage 键前缀 `plotgram.*` **不改**（避免用户丢本地草稿）

### 0.3 与 Studio 区分（对外一句话）

| 产品 | 一句话 |
|------|--------|
| **Editor** | 拿一张图，调好看，导出去——无需注册 |
| **Studio** | 用 Agent 画一张图，保存、迭代——需账号（SaaS） |

---

## 1. 问题与目标

### 1.1 现状问题

| 问题 | 表现 |
|------|------|
| 职责重叠 | 内置 `examples.ts`（20+ 示例），与 `showcase/` 画廊重复维护 |
| 入口混乱 | 首次打开加载默认示例；用户不清楚「逛示例」和「调一张图」的区别 |
| 命名混淆 | Playground 像开发者 sandbox；与 Studio SaaS 边界不清 |
| 主次颠倒 | DSL 代码区占主栏，Inspector（主题/布局/导出）偏次要 |

### 1.2 产品矩阵（改造后）

```
Showcase  →  「看」— 能力画廊，建立信任
Editor    →  「调」— 拿一张图，调校外观与布局，导出（无账号）
Studio    →  「创」— Agent 对话、云保存、版本历史（SaaS）
```

**一句话定位 Editor：**

> 无需注册的浏览器出图调校台——从 Showcase 带入一张图，调主题与布局，导出 SVG/PNG。

Editor **不是** Studio 的轻量版，**不是**第二个示例库，**不是** Agent 画图入口。

### 1.3 改造目标

| 目标 | 验收标准 |
|------|----------|
| 命名统一 | 对外全部为 Editor；`/editor` 为主路径；`/playground` 可重定向 |
| 职责单一 | 示例浏览只在 Showcase；Editor 无内置示例抽屉 |
| 冷启动可理解 | 默认空白或从 URL 载入；空白态有明确 CTA |
| 调校为主 | 预览 + Inspector 占主屏；DSL 代码区默认折叠 |
| Showcase 联动 | 画廊详情可「在 Editor 中打开」 |
| 与 Studio 分界 | 无 Agent、无云存、localStorage 仅本地暂存 |

---

## 2. 用户路径

### 2.1 主路径

```
Showcase 选中样例
    → 点击「在 Editor 中打开」
    → /editor?pgm=architecture/product.microservices.pgm
    → 调校主题 / 笔触 / 布局 / 导出
    → 下载 SVG/PNG 或复制分享链接（#s=）
```

### 2.2 次要路径

| 路径 | 行为 |
|------|------|
| 直接打开 `/editor` | 空白态 + CTA（去 Showcase / 粘贴 DSL / 上传 .pgm） |
| 旧链接 `/playground` | 重定向到 `/editor`（query / hash 保留） |
| 分享链接 `#s=` | 载入已压缩的 code + layout + appearance（现有 `share.ts`） |
| 开发者 | 展开左侧 DSL 区，使用 AST / Lint / Scene JSON |

### 2.3 与 Studio 的衔接（P2，可选）

Editor 导出或空白态底部弱引导：

> 需要 Agent 帮你画图、保存到云端？→ [打开 Studio](https://studio.plotgram.app)

不在 P0 实现 Studio 账号打通。

---

## 3. URL 与路由规范

### 3.1 先搞清楚：`pgm` 不是 DSL 正文

**常见误解**：以为 URL 里要塞进整段 Plotgram 源码（换行、中文、引号都要编码），会又长又难搞。

**实际设计**：`?pgm=` 只传 **showcase 里某个 `.pgm` 文件的路径**（一个短字符串），Editor 打开后再 **自己去 fetch 文件内容**。DSL 全文走 HTTP 响应体，**不进 URL**。

```
Showcase 点「在 Editor 中打开」
        │
        ▼
/editor?pgm=architecture/product.microservices.pgm
        │                    │
        │                    └── 只是路径，约 40 个字符
        ▼
Editor 内：fetch('/showcase/architecture/product.microservices.pgm')
        │
        ▼
得到完整 DSL 文本（可有任意换行、中文、多长都行）
        │
        ▼
渲染 + 调校
```

因此 **不存在**「DSL 换行怎么 encode」「DSL 太长 URL 装不下」等问题——那些是「把 DSL 塞进 query 参数」才会遇到的坑，我们** deliberately 不做那种设计**。

### 3.2 三种载入方式（各司其职）

| 方式 | URL 里放什么 | 是否含 DSL 正文 | 换行/中文/长度 | 典型场景 |
|------|-------------|----------------|----------------|----------|
| **`?pgm=路径`** | `flowchart/product.user-auth.pgm` | ❌ 只有路径 | ✅ 无影响（路径很短） | Showcase → Editor |
| **`#s=压缩包`** | LZ 压缩的 JSON（含 code + 主题 + 布局） | ✅ 含 DSL | ⚠️ 实用建议 DSL &lt; ~10KB | 分享「已调好」的图 |
| **粘贴 / 上传** | 无（内容在编辑器内） | ✅ 含 DSL | ✅ 无限制 | 用户自己的 `.pgm` |
| ~~`?source=明文`~~ | ~~整段 DSL~~ | — | ❌ **禁止** | 易超长、难编码、进 access log |

**记忆口诀**：

- **`pgm` = 指针**（指向仓库里的哪个文件）
- **`#s` = 快照**（把当前调校结果打包分享）
- **粘贴/上传 = 本地内容**（不进 URL）

### 3.3 路径与参数一览

| 机制 | 格式 | 用途 |
|------|------|------|
| **Canonical 路径** | `/editor` | 主入口 |
| **兼容路径** | `/playground` → `/editor` | 旧书签、文档链接 |
| **样例载入** | `?pgm=<showcase 相对路径>` | Showcase → Editor（**仅路径**） |
| **状态分享** | `#s=<LZ 压缩 JSON>` | 分享调校后的 code + layout + appearance |

示例：

```text
# 从 Showcase 打开：URL 里只有文件路径
/editor?pgm=architecture/product.microservices.pgm

# 调校后分享：路径可保留，hash 里带压缩后的完整状态
/editor?pgm=flowchart/product.user-auth.pgm#s=N4Igzg...

# 旧链接重定向
/playground?pgm=...   → 301 到 /editor?pgm=...
```

### 3.4 `pgm` 参数规则（路径白名单）

- 路径相对于 `showcase/` 根目录，必须以 `.pgm` 结尾
- 仅允许白名单前缀：`flowchart/`、`sequence/`、`architecture/`、`state/`、`er/`、`mindmap/`
- 禁止 `..` 路径穿越
- 加载失败时 toast 提示，回退空白态
- **禁止**用 `pgm` 或其它 query 参数传递 DSL 明文或 Base64 正文

### 3.5 部署与 Vite `base`

| 环境 | `base` | 说明 |
|------|--------|------|
| 开发 | `/editor/` | `vite.config.ts` 改为 `/editor/` |
| 生产 | `/editor/` | `deploy-*.sh` 部署到 `editor/` 目录或反向代理 |
| 兼容 | `/playground/` | Nginx / CDN 规则：`/playground` → `/editor` |

分享 URL 生成（`share.ts`）使用 `window.location.pathname`，重定向后自动落在 `/editor`。

---

## 4. 界面改造

### 4.1 布局优先级（桌面端）

```
┌────────────────────────────────────────────────────────────┐
│ TopBar：Plotgram Editor · 文件名 · 导出 · 分享 · 主题       │
├──────────┬─────────────────────────────┬───────────────────┤
│ DSL 区   │        大图预览              │   Inspector       │
│ 默认折叠 │   （graph / ascii 等 tab）   │  主题·笔触·布局   │
│ 可展开   │                             │  导出·背景        │
├──────────┴─────────────────────────────┴───────────────────┤
│ 底栏（折叠）：Problems · AST · Lint · Scene JSON            │
└────────────────────────────────────────────────────────────┘
```

移动端：预览优先，Inspector 第二屏，DSL 区第三屏。

### 4.2 空白态（Empty State）

无 `?pgm=`、无 `#s=`、localStorage 无有效内容时展示：

```
┌─────────────────────────────────────────┐
│         从 Showcase 选一张图开始          │
│    [浏览示例画廊]                        │
│                                         │
│    或：[粘贴 DSL]  [上传 .pgm]          │
│                                         │
│    （P2）[用 Studio 让 Agent 帮你画]     │
└─────────────────────────────────────────┘
```

- 不再默认注入 `DEFAULT_EXAMPLE_ID` 或 `BLANK_CODE` 骨架
- 清除首次访问对默认示例的依赖

### 4.3 Inspector 强化（P0）

- 主题 / 笔触皮肤（Standard → Excalidraw → Blueprint 一键切换）
- 布局算法、方向、边路由
- 预览背景、栅格导出倍率
- 导出：SVG / PNG / WebP / draw.io

### 4.4 移除或降级

| 组件 / 能力 | 处理 |
|-------------|------|
| `ExampleDrawer` / `ExampleDialog` | **移除** |
| `data/examples.ts` 精选列表 | **移除**；`?pgm=` fetch showcase 替代 |
| `DEFAULT_EXAMPLE_ID` 首次加载 | **移除** |
| `examplesGuideSeen` 脉冲引导 | **移除** |
| AST / Lint / Scene JSON | **保留**，收进底栏或「开发者」区 |
| Command Palette「切换示例」 | **移除** |

---

## 5. Showcase 联动改造

### 5.1 `showcase/index.html`

```html
<a class="primary" id="detailOpenEditor" target="_blank" rel="noopener">
  在 Editor 中打开
</a>
```

```javascript
const editorBase = '/editor';
detailOpenEditor.href = `${editorBase}?pgm=${encodeURIComponent(sample.path)}`;
```

### 5.2 画廊侧栏

```html
<a href="/editor">Editor · 图表调校</a>
```

### 5.3 全站导航（website）

`website/src/components/Layout.tsx` 等处的 Playground 链接改为 **Editor**，指向 `/editor`。

---

## 6. 技术任务分解

### Phase A — 命名与路由（P0）

| ID | 任务 | 文件 / 模块 | 验收 |
|----|------|-------------|------|
| A0 | Vite `base` 改为 `/editor/` | `playground/vite.config.ts`，`baseUrl.ts` | dev/prod 资源路径正确 |
| A0b | `/playground` → `/editor` 重定向 | 部署脚本 / Nginx / SPA fallback | 旧 URL 可访问 |
| A1 | 解析 `?pgm=`，fetch showcase 文件 | `App.tsx`，`lib/loadPgm.ts` | `/editor?pgm=...` 可渲染 |
| A2 | 移除默认示例加载 | `App.tsx` | 冷启动无示例 |
| A3 | 空白态 UI | `EmptyState.tsx` | 无参数时显示 CTA |
| A4 | `pgm` 白名单校验 | `loadPgm.ts` | 非法路径拒绝 |
| A5 | Showcase「在 Editor 中打开」 | `showcase/index.html` | 链到 `/editor?pgm=` |
| A6 | 页面标题 / TopBar 改为 Editor | `index.html`，`TopBar.tsx` | 无 Playground / Studio 字样 |

### Phase B — 界面重心（P0）

| ID | 任务 | 文件 / 模块 | 验收 |
|----|------|-------------|------|
| B1 | DSL 区默认折叠 | `App.tsx` | 主视图为预览 + Inspector |
| B2 | 移除 ExampleDrawer | `ExampleDrawer.tsx`，`App.tsx` | 无示例抽屉 |
| B3 | 精简 `examples.ts` | `data/examples.ts` | 仅保留 Inspector 所需类型 |
| B4 | 空白态：粘贴 / 上传 | `EmptyState.tsx` | 可手动载入 |
| B5 | 更新 `playground/README.md` 首段 | 说明对外名 Editor、目录仍 playground | 文档一致 |

### Phase C — 体验抛光（P1）

| ID | 任务 | 验收 |
|----|------|------|
| C1 | 笔触皮肤首屏快捷切换 | 3 秒内可见 Excalidraw 效果 |
| C2 | `?pgm=` 载入同步 filename 与 layout 默认 | Inspector 默认值正确 |
| C3 | 分享链接复制体验 | 调校后一键分享 |
| C4 | `website/` 导航与文案统一 Editor | 全站无 Playground 用户文案 |
| C5 | `deploy-playground.sh` 注释或别名 `deploy-editor` | 部署文档更新 |
| C6 | `studio/README.md` 对比表：Playground → Editor | 分工表一致 |

### Phase D — 与 Studio 衔接（P2）

| ID | 任务 | 验收 |
|----|------|------|
| D1 | 空白态 / 导出区 Studio 引导 | 外链可配置 |
| D2 | 评估 `playground/` 目录 rename → `editor/` | 按需，非必须 |

---

## 7. 不在本次范围

- Editor 合并进 `studio/` 或改称 Studio
- Editor 内嵌 Agent / LLM（归属 Studio SaaS）
- 云保存、账号体系
- PNG 内嵌 DSL metadata（另见企业路线图）
- 强制 `playground/` 目录 rename（P2 可选）

---

## 8. 风险与对策

| 风险 | 对策 |
|------|------|
| 旧 `/playground` 链接失效 | 301 重定向，保留 query/hash |
| Vite `base` 变更导致资源 404 | 同步改 deploy、website wasm 路径引用 |
| 空白页无 Wow | Showcase 主路径 + 笔触切换 |
| localStorage 旧草稿 | 键名不改；有 `?pgm=` / `#s=` 优先 |
| 「Editor」与 DSL 代码区混淆 | Inspector 为主；侧栏称「DSL」非「代码编辑器」 |
| 与 Studio README 不一致 | Phase C6 同步更新 |

---

## 9. 验收清单（总）

- [x] 对外文案全部为 **Editor**（无 Playground 用户可见字样）
- [x] `/editor` 为主路径；`/playground` 可重定向
- [x] 冷启动无内置示例，显示空白态或 CTA
- [x] Showcase 可「在 Editor 中打开」
- [x] Inspector 为主流程，DSL 区默认折叠
- [x] `#s=` 分享仍可用
- [x] 无 ExampleDrawer / examples 精选列表
- [x] UI 不用 Studio 指代 Editor
- [x] `studio/README.md` 分工表已更新

---

## 10. 修订记录

| 版本 | 日期 | 说明 |
|------|------|------|
| 0.1.0-draft | 2026-07-22 | 初稿：定位、路径、任务分解 |
| 0.2.0-draft | 2026-07-22 | 对外更名为 Editor；`/editor` 主路径；§0 命名规范 |
| 0.2.1-draft | 2026-07-22 | §3.1 明确 `?pgm=` 为文件路径引用，非 DSL 正文 |
