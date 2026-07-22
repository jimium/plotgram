# Plotgram Editor（代码目录：`playground/`）

对外产品名 **Editor**——无需注册的浏览器出图调校台（调主题/布局、导出 SVG/PNG）。  
代码目录暂保留 `playground/`，与 SaaS 产品 **Studio**（Agent 画图）区分。

基于 React + WASM，可在浏览器中载入 `.pgm`、调校并渲染图表。改造任务见 [Editor 改造方案](../docs/方案计划/Playground改造方案-2026-07.md)。

## 快速开始

### 前置需求

- Node.js 16+
- npm 或 yarn

### 启动开发服务器

主题与渲染逻辑在 Rust/WASM 里，**改 `plotgram-core` 后必须重新编译 WASM**。请用：

```bash
cd playground
npm install
./start.sh
```

`start.sh` 会先 `wasm-pack build` 到 `plotgram-wasm/`，再启动 Vite（固定 **3000** 端口）。

**不要**在 `public/plotgram-wasm/` 放 wasm 副本——Vite 会优先提供 `public/` 下的文件，导致浏览器加载过期 wasm（可与 `plotgram-wasm/plotgram_wasm_bg.wasm` 的 md5 对比排查）。

仅 `npm run dev` 不会更新 WASM，强刷也可能因上述路径劫持看不到新主题。

然后在浏览器中打开 [http://localhost:3000/](http://localhost:3000/)

### 功能介绍

- 📝 **代码编辑**：左侧是 Plotgram DSL 编辑器，支持实时编辑
- 🎨 **实时预览**：右侧会实时渲染 SVG 格式的图表
- 🌐 **示例选择**：选择预设的示例快速体验
- 💾 **导出功能**：点击 "Export SVG" 下载 SVG 文件
- ✅ **错误提示**：实时显示解析和验证错误

### 目录结构

```
playground/
├── plotgram-wasm/    # 编译好的 WASM 模块
├── public/         # 静态资源
├── src/
│   ├── App.jsx     # 主应用组件
│   ├── App.css     # 样式文件
│   └── main.jsx    # 入口文件
├── index.html      # HTML 模板
├── vite.config.js  # Vite 配置
└── package.json
```

### 相关文档

- [Editor 改造方案](../docs/方案计划/Playground改造方案-2026-07.md)
- 项目 `/docs/` 目录：语法规范、架构设计、使用案例

## 其他 Plotgram 模块

- `plotgram-core` - 核心解析、验证和渲染库
- `plotgram-cli` - 命令行工具
- `plotgram-server` - 后端服务（开发中）
- `plotgram-wasm` - WASM 绑定

