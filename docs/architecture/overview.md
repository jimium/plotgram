# Drawify 整体架构

Drawify 是一个多语言、多平台的图表 DSL 系统，采用模块化设计，核心功能与各种绑定和应用分离。

## 系统架构图

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                            应用层 (Applications)                            │
├─────────────────────────┬─────────────────────────┬─────────────────────────┤
│   Web 编辑器 (React)   │   CLI 工具 (drawify-cli) │   后端服务 (server)    │
├─────────────────────────┴─────────────────────────┴─────────────────────────┤
│                             绑定层 (Bindings)                               │
├─────────────────────────────────────────────────────────────────────────────┤
│        WASM 绑定 (drawify-wasm)       │           (Future FFI)            │
├─────────────────────────────────────────────────────────────────────────────┤
│                             核心层 (Core)                                   │
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │  Parser  │  Validator  │  Layout Engines  │  Renderers  │  Diff/Patch  │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────────┘
```

## 核心模块详解

### 1. drawify-core
核心库，包含所有业务逻辑：

#### 子模块

- **Lexer & Parser** (`lexer.rs`, `parser.rs`)
  - 将 Drawify 代码解析为 AST
  - 包含诊断信息（错误位置、修复提示）

- **AST** (`ast.rs`)
  - 抽象语法树定义
  - 支持多种图表类型

- **Validation** (`validation/`)
  - 验证 Drawify 代码的语义正确性
  - 检查未定义实体、循环依赖等

- **Layout Engines** (`layout/`)
  - 自动布局多种图表
  - 层次布局、力导向布局、圆形布局等

- **Renderers** (`renderer/`)
  - SVG、PNG、WebP、ASCII 等多种格式渲染
  - 图表类型特定的渲染器（流程图、序列图、架构图等）

- **Diff & Patch** (`diff.rs`)
  - 两个 Drawify 代码的差异比较
  - 增量式更新支持

### 2. drawify-cli
命令行工具，提供核心功能的 CLI 访问：

```bash
drawify-cli render --format svg input.dfy > output.svg
drawify-cli validate input.dfy
```

### 3. drawify-wasm
WebAssembly 绑定，用于浏览器或 Node.js 环境：

```javascript
import init, { render } from './drawify-wasm';

await init();
const result = render('diagram flowchart { ... }');
```

### 4. Web 编辑器
React + Vite 构建的 Web 应用：
- 实时编辑 Drawify 代码
- 实时预览渲染结果
- 导出 SVG 功能

## 数据流向

```
Drawify 代码
    ↓
[Lexer] → Token Stream
    ↓
[Parser] → AST + 诊断信息
    ↓
[Validator] → 验证结果 + 警告/错误
    ↓
[Layout] → 位置坐标信息
    ↓
[Renderer] → SVG/PNG/WebP/ASCII
```

## 设计原则

1. **模块化**：核心与绑定分离，可独立演进
2. **可扩展**：新增图表类型、布局算法、渲染格式都易于实现
3. **高性能**：Rust 实现，支持 WASM，在浏览器中也能快速运行
4. **良好的诊断**：提供详细的错误位置和修复提示
5. **稳定性**：严格的验证和错误处理

## 未来扩展

- Python/Node.js 等语言的 FFI 绑定
- 实时协作功能
- 插件系统支持自定义渲染

