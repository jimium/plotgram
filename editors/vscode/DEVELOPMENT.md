# Drawify VS Code 扩展 — 本地开发与调试

在 VS Code 里查看插件效果，标准做法是使用 **Extension Development Host**（扩展开发宿主窗口）。

## 1. 打开扩展目录

用 VS Code 打开 **`editors/vscode`** 这个文件夹（不要只打开整个 monorepo 根目录，否则 F5 可能找不到扩展）。

```
文件 → 打开文件夹 → 选择 flowml/editors/vscode
```

## 2. 安装依赖并构建（首次）

在终端里执行：

```bash
cd editors/vscode
npm install
npm run build
```

`npm run build` 会依次执行：

1. `npm run build:wasm` — 从 `crates/drawify-wasm` 独立打包 WASM 到 `media/wasm/`（Webview）和 `media/node/`（扩展宿主诊断）
2. `npm run compile` — 编译 TypeScript

> 需要已安装 `wasm-pack`：`cargo install wasm-pack`

## 3. 启动调试（F5）

1. 左侧点 **运行和调试**（或 `Cmd+Shift+D` / `Ctrl+Shift+D`）
2. 选择 **Run Extension**
3. 按 **F5**

会弹出一个新的 VS Code 窗口，标题栏带有 **`[Extension Development Host]`** 标记，扩展在这个窗口里生效。

## 4. 在新窗口里测试

### 语法高亮

打开任意 `.dfy` 文件，例如（相对于 monorepo 根目录）：

```
showcase/flowchart/s.linear-chain.dfy
```

在新窗口里通过 **文件 → 打开** 选择该文件即可。应能看到 `diagram`、`entity`、`->` 等关键字着色。

### 图形预览

1. 打开 `.dfy` 文件
2. 点击编辑器右上角的 **预览图标**，或 `Cmd+Shift+P` / `Ctrl+Shift+P` → 输入 **Open Drawify Preview**
3. 右侧预览面板会通过本地 WASM 渲染 SVG；编辑文件后约 150ms 自动刷新

### 语法/语义诊断

打开 `.dfy` 文件后，扩展宿主通过 `media/node/` 中的 WASM 调用 `validate`，在编辑器中显示错误下划线，并同步到 **Problems** 面板。

### Markdown 内嵌

1. 新建或打开 `.md` 文件，写入：

````markdown
```drawify
diagram flowchart {
    entity start "Start" { type: start }
    entity end "End" { type: end }
    start -> end
}
```
````

2. `Cmd+Shift+V` / `Ctrl+Shift+V` 打开 Markdown 预览
3. `drawify` 代码块由 **markdown-it 插件**在扩展宿主侧离线渲染为 SVG（与 `.dfy` 诊断共用 `media/node/` WASM）

---

## 开发技巧

| 操作 | 说明 |
|------|------|
| 改代码后重载 | 在 **Extension Development Host** 窗口按 `Cmd+Shift+P` → **Developer: Reload Window** |
| 自动编译 | 在 `editors/vscode` 终端运行 `npm run watch`，改 TypeScript 会自动编译 |
| 更新 WASM | 修改 `drawify-core` / `drawify-wasm` 后执行 `npm run build:wasm` |
| 查看扩展日志 | 在原窗口（开发窗口）→ **查看 → 输出** → 下拉选择 **Log (Extension Host)** |

---

## 常见问题

### F5 没反应或找不到配置？

确认当前打开的是 `editors/vscode` 文件夹；项目已包含 `.vscode/launch.json`，选择 **Run Extension** 即可。

### 高亮没出来？

确认文件扩展名是 `.dfy`，且编辑器右下角语言模式显示 **Drawify**。

### WASM 加载失败（Failed to fetch）

1. 确认已执行 `npm run build:wasm`，且 `media/wasm/` 下存在 `drawify_wasm.js` 与 `drawify_wasm_bg.wasm`
2. 在 Extension Development Host 窗口执行 **Developer: Reload Window** 后重开预览
3. 若仍失败，查看 **输出 → Log (Extension Host)** 是否有路径或 CSP 相关报错

---

## 打包与分发（给测试者安装）

日常开发用 F5 即可；要把扩展发给他人离线安装，需要打成 **`.vsix`** 安装包。

### 前置条件

打包前必须完成完整构建（含 WASM），否则测试者安装后会缺少渲染能力：

```bash
cd editors/vscode
npm install
npm run build
```

确认以下目录存在且非空：

- `out/` — 编译后的扩展入口
- `media/wasm/` — Webview 预览用 WASM
- `media/node/` — 诊断与 Markdown 渲染用 WASM

### 生成 .vsix

在 `editors/vscode` 目录执行：

```bash
npx @vscode/vsce package
```

成功后当前目录会出现类似 `drawify-0.1.0.vsix` 的文件（文件名随 `package.json` 里的 `name` 和 `version` 变化）。

`package.json` 已配置 `vscode:prepublish`，`vsce` 打包时会自动执行 `npm run build`（含 WASM 构建）。若你本地已构建过，也可直接打包。

> 若提示找不到 `vsce`，使用 `npx @vscode/vsce` 即可，无需全局安装。

### 分发给测试者

将 `.vsix` 文件发给测试者（网盘、GitHub Release、内部分享链接等均可）。

### 测试者安装步骤

1. 打开 VS Code
2. **扩展** 视图 → 右上角 **⋯** → **从 VSIX 安装…**（Install from VSIX…）
3. 选择收到的 `drawify-x.y.z.vsix`
4. 按提示 **Reload** / 重启 VS Code

安装后验证：

- 打开 `.dfy` 文件 → 语法高亮、Problems 诊断、预览按钮
- 打开含 ` ```drawify ` 代码块的 `.md` → Markdown 预览中显示 SVG

### 卸载

**扩展** 视图 → 找到 **Drawify** → **卸载**。

### 打包注意事项

| 项 | 说明 |
|----|------|
| 版本号 | 每次发给测试者前，在 `package.json` 的 `version` 递增（如 `0.1.0` → `0.1.1`），便于区分 |
| 体积 | WASM 约 4MB+，`.vsix` 体积偏大属正常 |
| 平台 | 扩展在 Windows / macOS / Linux 的 VS Code 桌面版均可安装；WASM 随包分发，无需测试者单独装 Rust |
| 不要提交 `.vsix` | 已在 `.gitignore` 忽略；二进制安装包不宜进 Git |

---

## 发布到 VS Code Marketplace（将来）

以下流程在扩展功能稳定、准备公开分发时使用。

### 1. 注册 Publisher

1. 用 Microsoft 账号登录 [Visual Studio Marketplace 管理页](https://marketplace.visualstudio.com/manage)
2. 点击 **Create Publisher**
3. 填写 **Publisher ID**（例如 `your-org-drawify`，全局唯一、创建后不可改）
4. 填写显示名称等基本信息

### 2. 更新 package.json

发布前核对并补全扩展元数据，至少包括：

```json
{
  "publisher": "your-publisher-id",
  "name": "drawify",
  "displayName": "Drawify",
  "version": "0.1.0",
  "description": "...",
  "license": "MIT",
  "repository": {
    "type": "git",
    "url": "https://github.com/your-org/flowml"
  },
  "bugs": {
    "url": "https://github.com/your-org/flowml/issues"
  },
  "homepage": "https://github.com/your-org/flowml#readme",
  "icon": "media/icon.png",
  "categories": ["Programming Languages", "Visualization"]
}
```

说明：

- **`publisher`** 必须与 Marketplace 上的 Publisher ID 一致（当前仓库里是占位值 `drawify`，发布前要改成你的真实 ID）
- **`icon`** 建议提供 128×128 PNG（路径相对扩展根目录）
- Marketplace 详情页会读取扩展根目录的 **`README.md`**（可选但强烈建议），可新建 `editors/vscode/README.md` 专用于市场展示

### 3. 创建 Personal Access Token（PAT）

1. 打开 [Azure DevOps](https://dev.azure.com) → 右上角用户菜单 → **Personal access tokens**
2. **New Token**
3. Scope 选择 **Custom defined** → **Marketplace** → **Manage**（发布扩展所需权限）
4. 复制生成的 token（只显示一次，请妥善保存）

### 4. 登录 vsce 并发布

```bash
cd editors/vscode
npm run build
npx @vscode/vsce login your-publisher-id
# 粘贴 PAT

npx @vscode/vsce publish
```

首次发布会交互确认 Publisher；也可一步指定：

```bash
npx @vscode/vsce publish -p <YOUR_PAT>
```

### 5. 后续版本更新

1. 修改代码并完成 `npm run build` 验证
2. 递增 `package.json` 的 `version`（遵循 [语义化版本](https://semver.org/lang/zh-CN/)）
3. 执行 `npx @vscode/vsce publish`

用户已在 Marketplace 安装的扩展会在 VS Code 扩展视图中收到更新提示。

### 6. 发布前检查清单

- [ ] `npm run build` 成功，F5 与 `.vsix` 本地安装均验证通过
- [ ] `publisher`、`repository`、`license` 元数据正确
- [ ] `README.md`、图标、截图（可选）已准备
- [ ] `package.json` 的 `engines.vscode` 最低版本合理（当前 `^1.85.0`）
- [ ] 无敏感信息（token、内网地址等）被打包进扩展

### 7. 私有 / 内部分发替代方案

若暂不上架公开市场，可长期使用 **`.vsix` 分发**（见上一节），或考虑：

- GitHub Release 附带 `.vsix` 资产
- 私有 Extension Gallery（企业版 Azure DevOps Server 场景）

---

## 当前可验证范围

| 功能 | 状态 |
|------|------|
| `.dfy` 语法高亮 | 可用 |
| 语法/语义诊断 | 可用（WASM validate） |
| `.dfy` 图形预览 | 可用（WASM render） |
| Markdown `drawify` 代码块 | 可用（WASM render） |

需求说明见 [REQUIREMENTS.md](./REQUIREMENTS.md)。
