# SVG Debug 元数据使用指南

开发阶段可在 SVG 中嵌入 `data-dfy-*` 属性，把 DOM 元素映射回 DSL 实体，便于 DevTools 调试和脚本分析。

> 实现：`crates/drawify-core/src/render/paint/svg_debug.rs`  
> 由 Cargo feature **`svg-debug`** 控制（`drawify-core` 默认开启）。

---

## 如何开启 / 关闭

```toml
# drawify-core/Cargo.toml
[features]
default = ["raster", "svg-debug"]
```

```bash
# 关闭 debug 元数据（发布构建）
cargo build -p drawify-cli --no-default-features --features raster
```

`drawify-wasm` 默认**不**启用 `svg-debug`，以减小产物体积。

---

## 输出结构

根元素：

```xml
<svg ... data-dfy-debug="1">
```

每个逻辑元素外包一层 `<g>`：

```xml
<g data-dfy-kind="node" data-dfy-id="api" data-dfy-source-line="12">
  <rect .../>
  <text .../>
</g>

<g data-dfy-kind="edge" data-dfy-index="2"
   data-dfy-from="api" data-dfy-to="db" data-dfy-arrow="active">
  <path .../>
</g>

<g data-dfy-kind="edge-label" data-dfy-index="2" data-dfy-from="api" data-dfy-to="db">
  <text .../>
</g>

<g data-dfy-kind="group" data-dfy-id="backend">
  <rect .../>  <!-- 分组框 -->
</g>
```

### 属性说明

| 属性 | 出现位置 | 含义 |
|------|----------|------|
| `data-dfy-debug` | `<svg>` | 调试模式标记 |
| `data-dfy-kind` | `<g>` | `node` / `edge` / `edge-label` / `group` |
| `data-dfy-id` | node、group | 对应 `entity.id` 或 `group.id` |
| `data-dfy-index` | edge | `relations` 向量下标 |
| `data-dfy-from` / `data-dfy-to` | edge | 边端点实体 id |
| `data-dfy-arrow` | edge | `active` / `passive` / `bidirectional` |
| `data-dfy-source-line` | node、group、edge | DSL 源码行号（`span.line > 0` 时） |

---

## 浏览器中调试

1. `drawify render diagram.dfy -o out.svg`
2. 在浏览器打开 SVG（或嵌入页面）
3. DevTools 检查元素 → 查看 `data-dfy-*` → 对照 `.dfy` 源文件

---

## 与 LayoutLint 配合

| 能力 | 数据源 | 用途 |
|------|--------|------|
| LayoutLint | `LayoutResult` 几何 | 违规列表、CI |
| `data-dfy-*` | 渲染后 SVG | 可视化定位 |

Lint **不解析** SVG；将来可在违规报告的 group 上附加 `data-dfy-lint` 高亮（尚未实现）。

工作流建议：

```bash
drawify lint diagram.dfy --format json > lint.json
drawify render diagram.dfy -o diagram.svg
# 根据 lint.json 的 entity_ids / edge_index 在 DevTools 中查找对应 <g>
```

---

## 脚本示例

```javascript
// 在浏览器控制台：列出所有边及其 DSL 映射
[...document.querySelectorAll('[data-dfy-kind="edge"]')].map(g => ({
  index: g.dataset.dfyIndex,
  from: g.dataset.dfyFrom,
  to: g.dataset.dfyTo,
}));
```

---

## 相关文档

- [layout-lint.md](layout-lint.md) — 布局质量检查
- [drawify-cli.md](drawify-cli.md) — `render` 命令
