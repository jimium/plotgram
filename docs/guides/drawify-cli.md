# drawify CLI 使用指南

`drawify` 是 Drawify 的命令行入口，封装 parse → prepare → validate → layout → render 管线。

> 实现：`crates/drawify-cli/`

---

## 安装与帮助

```bash
cargo build -p drawify-cli
cargo install --path crates/drawify-cli   # 可选

drawify --help
drawify render --help
```

---

## 命令总览

| 命令 | 作用 |
|------|------|
| `render` | 解析并渲染为 SVG / PNG / draw.io 等 |
| `validate` | 语法 + 语义校验 |
| `lint` | 布局静态质量检查（LayoutLint） |
| `export` | 导出 PreparedDiagram JSON（AST） |
| `diff` | 两份 DSL 的语义 diff（diff2） |
| `patch` | 将 ChangeSet 补丁应用到 DSL |

---

## `drawify render`

```bash
drawify render diagram.dfy
drawify render diagram.dfy -f png -o out.png
drawify render outline.md --input-format md-outline -f svg
```

### 常用参数

| 参数 | 说明 |
|------|------|
| `-f, --format` | 输出格式，默认 `svg` |
| `-o, --output` | 输出文件；省略则写 stdout |
| `--fonts-dir` | 字体目录（PNG/WebP 中文渲染）；也可用环境变量 `DRAWIFY_FONTS_DIR` |
| `--input-format` | 输入格式：`dfy`（默认）或 `md-outline` |

### 支持的输出格式

| `-f` 值 | 说明 | 备注 |
|---------|------|------|
| `svg` | 矢量图 | 默认 |
| `ascii` / `text` | 终端字符画 | |
| `png` | 位图 | 需字体目录 |
| `webp` | 位图 | 需字体目录 |
| `json` | PreparedDiagram JSON | 与 `export` 类似 |
| `drawio` | draw.io XML | |
| `md-outline` | Markdown 大纲 | 思维导图导出 |
| `opml` | OPML | 思维导图导出 |
| `freemind` / `mm` | FreeMind | 思维导图导出 |

### 输入格式

- **`.dfy`**：Drawify DSL（默认）
- **`.md` / `.markdown`**：无 `--input-format` 时自动按 Markdown 大纲导入思维导图
- **`--input-format md-outline`**：显式指定大纲导入

渲染前会跑完整 `parse_prepare_validate`；有错误则打印诊断并 `exit 1`。

---

## `drawify validate`

```bash
drawify validate diagram.dfy
drawify validate diagram.dfy --format json
drawify validate diagram.dfy --layout-check
```

| 参数 | 说明 |
|------|------|
| `--format` | `text`（默认）或 `json` |
| `--layout-check` | 通过后额外跑 LayoutLint **strict** 预设 |

文本模式会显示源码片段与 `^` 指示位置。详见 [layout-lint.md](layout-lint.md)。

---

## `drawify lint`

```bash
drawify lint diagram.dfy
drawify lint diagram.dfy --profile strict
drawify lint diagram.dfy --ignore edge_crossing
drawify lint diagram.dfy --format json --fail-on-warning
```

完整说明见 [layout-lint.md](layout-lint.md)。

---

## `drawify export`

```bash
drawify export diagram.dfy > ast.json
```

输出 **PreparedDiagram** 的 JSON（经 prepare，**不**跑 validate）。适合查看物化后的 AST、调试 theme 展开结果。

与 `render -f json` 的区别：`export` 用 `parse_prepare`；`render` 用 `parse_prepare_validate`，校验失败不输出。

---

## `drawify diff`

比较两份 `.dfy` 在 **RawDiagram** 层的语义差异（diff2，不含 theme 物化差异）。

```bash
drawify diff -o old.dfy -n new.dfy
drawify diff -o old.dfy -n new.dfy --format json > changes.json
```

文本输出示例：

```text
变更统计: +1 -0 ~2

+ /entity/c
~ /entity/a/label
~ /relation/a->b/arrow
```

JSON 输出为 `ChangeSet`，可供 `patch` 使用。详见 [diff-and-patch.md](diff-and-patch.md)。

---

## `drawify patch`

```bash
drawify patch diagram.dfy changes.json -o patched.json
drawify patch diagram.dfy changes.json   # stdout
```

- 补丁文件：`ChangeSet` JSON，或 `Change` 数组
- 在 RawDiagram 上应用补丁后 **prepare**，输出 PreparedDiagram JSON
- 部分失败时打印警告；全部失败 `exit 1`

---

## 退出码约定

| 场景 | 退出码 |
|------|--------|
| 成功 | 0 |
| 校验/渲染/lint 失败 | 1 |
| 文件读取失败 | 1 |

---

## 常见工作流

### 本地预览

```bash
drawify render showcase/flowchart/s.linear-chain.dfy -o /tmp/out.svg
```

### CI：语法 + 布局门禁

```bash
drawify validate diagram.dfy --layout-check
# 或
drawify lint diagram.dfy --profile strict
```

### Agent 改图

```bash
drawify diff -o base.dfy -n edited.dfy --format json > delta.json
# Agent 编辑 delta.json 或生成新 patch
drawify patch base.dfy delta.json -o result.json
```

### Showcase 批量（见 [showcase-workflow.md](showcase-workflow.md)）

```bash
./showcase/render-all.sh --validate
```

---

## 相关文档

- [render-pipeline.md](render-pipeline.md) — 管线阶段说明
- [layout-lint.md](layout-lint.md) — LayoutLint
- [diff-and-patch.md](diff-and-patch.md) — diff2 语义补丁
