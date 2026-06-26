# Showcase 回归工作流

`showcase/` 是按图表类型与复杂度组织的 `.dfy` 样例集，用于视觉对比、冒烟测试和布局质量回归。

> 样例目录说明：[showcase/README.md](../../showcase/README.md)

---

## 目录与命名

```text
showcase/
├── flowchart/     s.*  n.*  c.*
├── sequence/
├── architecture/
├── state/
├── er/
└── mindmap/
```

| 前缀 | 复杂度 | 节点规模 |
|------|--------|----------|
| `s.` | simple | ≤4 |
| `n.` | normal | 5–10 |
| `c.` | complex | 10+ |

---

## 批量渲染

```bash
# 全部渲染为 SVG（默认）
./showcase/render-all.sh

# SVG + PNG
./showcase/render-all.sh -a

# 渲染前 validate
./showcase/render-all.sh --validate

# 指定格式
./showcase/render-all.sh -f png
```

输出目录默认 `showcase/output/`；历史 SVG 快照见 `showcase/.history/`（见 showcase README）。

### 本地画廊

```bash
python3 -m http.server --directory showcase 4173
# 打开 http://localhost:4173/index.html
```

---

## 布局质量回归（LayoutLint）

### 单文件

```bash
drawify lint showcase/architecture/c.k8s-platform-stack.dfy --profile strict
```

### 批量（shell 示例）

```bash
fail=0
while IFS= read -r -d '' f; do
  if ! drawify lint "$f" --profile strict --format json >/dev/null; then
    echo "FAIL: $f"
    fail=1
  fi
done < <(find showcase -name '*.dfy' -print0)
exit $fail
```

| 场景 | 推荐 preset |
|------|-------------|
| CI 门禁 | `strict` |
| 日常排查 | `default` |
| 走廊贴边等 | `verbose` 或 `--ignore edge_crossing` |

详见 [layout-lint.md](layout-lint.md)。

---

## 冒烟测试（Rust）

```bash
cargo test -p drawify-core --test showcase_smoke
```

覆盖：parse → prepare → validate → render(SVG)，部分含 `validate_group_containment`。

---

## 算法评估（drawify-eval）

对 showcase 做布局/路由算法横向对比：

```rust
// 见 drawify-eval.md — EvalReport + presets::layout_comparison()
```

典型流程：

1. 遍历 `showcase/**/*.dfy`
2. `parse` → `EvalEngine::compare`
3. 输出 `report.md`，保存 `HistoryStore` 基线
4. 算法改动后 `engine.diff` 检测回归

---

## 推荐 CI 检查清单

| 步骤 | 命令 / 测试 |
|------|-------------|
| 语法语义 | `drawify validate` 或 `render-all.sh --validate` |
| 布局硬约束 | `drawify lint --profile strict` |
| 渲染不崩 | `showcase_smoke` / `render-all.sh` |
| 算法回归 | drawify-eval + `HistoryStore`（可选） |

---

## 添加新样例

1. 按类型放入对应目录，使用 `s.` / `n.` / `c.` 前缀
2. `drawify validate` + `drawify lint --profile strict`
3. `drawify render ... -o` 目视检查
4. 更新 `showcase/README.md` 代表样例表（若属重点场景）

---

## 相关文档

- [drawify-cli.md](drawify-cli.md)
- [layout-lint.md](layout-lint.md)
- [drawify-eval.md](drawify-eval.md)
