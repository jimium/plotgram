# Drawify WASM 编辑器

这是一个基于 React 和 WASM 的 Drawify 实时编辑器，可以在浏览器中编写和渲染图表。

## 快速开始

### 前置需求

- Node.js 16+
- npm 或 yarn

### 启动开发服务器

```bash
cd playground
npm install
npm run dev
```

然后在浏览器中打开 [http://localhost:3000/](http://localhost:3000/)

### 功能介绍

- 📝 **代码编辑**：左侧是 Drawify DSL 编辑器，支持实时编辑
- 🎨 **实时预览**：右侧会实时渲染 SVG 格式的图表
- 🌐 **示例选择**：选择预设的示例快速体验
- 💾 **导出功能**：点击 "Export SVG" 下载 SVG 文件
- ✅ **错误提示**：实时显示解析和验证错误

### 目录结构

```
playground/
├── drawify-wasm/    # 编译好的 WASM 模块
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

- [Playground 重新设计方案](../docs/product/playground-redesign.md)
- 项目 `/docs/` 目录：语法规范、架构设计、使用案例

## 其他 Drawify 模块

- `drawify-core` - 核心解析、验证和渲染库
- `drawify-cli` - 命令行工具
- `drawify-server` - 后端服务（开发中）
- `drawify-wasm` - WASM 绑定

