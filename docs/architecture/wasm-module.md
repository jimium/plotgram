# WASM 模块设计

## 概述

`drawify-wasm` 是 `drawify-core` 的 WebAssembly 绑定，使其能在浏览器或 Node.js 环境中运行，无需后端服务。

## 模块架构

```
┌─────────────────────────────────────────────────────────────────┐
│                        JavaScript 层                            │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │  React App (Editor + Preview)                            │  │
│  └───────────────────────────────────────────────────────────┘  │
│                            ↓                                    │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │  drawify-wasm.js (wasm-bindgen 生成的绑定)                 │  │
│  └───────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│                      WebAssembly (WASM) 层                      │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │  drawify-wasm (Rust + wasm-bindgen)                       │  │
│  └───────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│                        drawify-core                             │
│  [Parser → Validator → Layout → Renderer]                       │
└─────────────────────────────────────────────────────────────────┘
```

## API 设计

### 核心函数

```rust
#[wasm_bindgen]
pub fn render(source: &str) -> String;

#[wasm_bindgen]
pub fn validate(source: &str) -> String;

#[wasm_bindgen]
pub fn version() -> String;
```

### 返回值格式

#### Render 结果

```json
{
  "success": true,
  "svg": "<svg>...</svg>",
  "errors": [],
  "warnings": []
}
```

错误时返回：

```json
{
  "success": false,
  "svg": null,
  "errors": ["解析错误: ...", "验证错误: ..."],
  "warnings": ["警告: ..."]
}
```

## 构建流程

### 1. 开发构建

```bash
cd crates/drawify-wasm
wasm-pack build --target web --out-dir ../../playground/drawify-wasm
```

### 2. 发布构建

```bash
wasm-pack build --target web --release
```

### 优化配置 (Cargo.toml)

```toml
[profile.release]
lto = true
opt-level = "z"
codegen-units = 1
```

## Vite 集成

### vite.config.js 配置

```javascript
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  optimizeDeps: {
    exclude: ['drawify-wasm']  // 不预优化 WASM 包
  },
  server: {
    port: 3000
  }
});
```

### 在 React 中的加载方式

```javascript
const loadWasm = async () => {
  const wasmModule = await import('../drawify-wasm/drawify_wasm.js');
  await wasmModule.default(); // 初始化 WASM
  // 现在可以使用 wasmModule.render(), wasmModule.validate() 等
};
```

## 错误处理与 panic hook

启用 `console_error_panic_hook` 以便在浏览器中获得更好的错误跟踪：

```rust
#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}
```

## 性能考虑

1. **加载时间**：WASM 模块较大（~1-2MB），建议启用 gzip/brotli 压缩
2. **运行时**：解析和渲染都在 WASM 中完成，性能优秀
3. **防抖**：在频繁输入时使用防抖减少渲染次数

## 内存安全

WASM 模块有独立的内存空间，与 JavaScript 隔离，保证了安全性。

