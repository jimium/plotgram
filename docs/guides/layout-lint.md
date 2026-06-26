# LayoutLint 使用指南

LayoutLint 在布局计算完成后，对 `LayoutResult` 运行一组**确定性几何规则**，输出可追溯到 DSL 实体（`entity.id`、`edge index`、`group.id`）的违规列表。

> 实现位置：`crates/drawify-core/src/layout/lint/`

---

## 核心概念

### 检查什么、不检查什么

| 检查 | 不检查 |
|------|--------|
| 节点 / 分组重叠 | SVG DOM 结构 |
| 节点 / 子组是否越出分组边界 | 视觉样式、主题 |
| 边是否穿过非端点节点 | DSL 语法 / 语义（由 `validate` 负责） |
| 边是否穿过**无关**分组内部 | |
| 边交叉、贴 group 边框（可选） | |

数据流：

```text
.dfy → parse → prepare → compute_layout → LayoutResult
                                              ↓
                                        LayoutLinter::run
                                              ↓
                                         LintReport
```

**不解析 SVG**。几何来自 `LayoutResult` 中的节点 bbox、分组 bbox、边路径折线。

### 与 drawify-eval 的区别

| | LayoutLint | drawify-eval |
|--|------------|--------------|
| 输出 | 逐条违规 + 归因 | 聚合分数、算法对比、回归 |
| 用途 | 开发调试、CI 硬门禁 | 算法 A/B、质量趋势 |
| 位置 | `drawify-core::layout::lint` | `drawify-eval` crate |

两者可配合：lint 发现「哪里错了」，eval 回答「整体好不好」。

### 严重级别

| 级别 | 含义 | 默认是否导致失败 |
|------|------|------------------|
| `error` | 硬几何错误 | 是（`is_clean()` / `is_acceptable`） |
| `warning` | 质量信号，可能难免 | 否（除非 `--fail-on-warning`） |

---

## 内置规则

规则 ID 为 snake_case 字符串，用于 CLI `--ignore` 与 JSON 报告的 `rule` 字段。

| 规则 ID | 默认级别 | 说明 |
|---------|----------|------|
| `node_overlap` | error | 两节点 AABB 重叠 |
| `group_overlap` | error | 两分组 AABB 重叠（**排除**父子嵌套关系） |
| `node_outside_group` | error | 节点超出所属分组边界 |
| `child_group_outside_parent` | error | 子分组超出父分组边界 |
| `edge_through_node` | error | 边路径穿过非起终节点（折线 ≥3 段时跳过首尾 stub） |
| `edge_crosses_group_interior` | error | 边穿过某分组内部，且该分组不在端点祖先链上 |
| `edge_crossing` | warning | 两两边在非共享端点处交叉 |
| `edge_on_group_border` | warning | 边路径与分组边框重合（正交走廊路由常触发，**default 预设默认关闭**） |

### 关于「合法穿越」

部分现象是布局算法**刻意允许**的，lint 已做豁免或默认关闭：

1. **父子 group 重叠**：子组在父组内部，AABB 必然重叠 → `group_overlap` 跳过祖先后代对。
2. **穿过祖先 group 内部**：同容器内节点互连，路径常走父 group 空白区 → `edge_crosses_group_interior` 跳过端点所属 group 的**祖先链**。
3. **沿 group 边框走线**：正交路由的走廊行为 → `edge_on_group_border` 在 `default` 预设中默认**不启用**；需要时用 `--profile verbose` 观察。
4. **边交叉**：稠密图可能不可避免 → 默认 `warning`，`strict` 预设不检查。

---

## 预设（Profile）

通过 `LintConfig::profile` 或 CLI `--profile` 选择。

| Profile | CLI 别名 | 启用规则 | 典型场景 |
|---------|----------|----------|----------|
| `default` | — | 全部 except `edge_on_group_border`；含 `edge_crossing`(warning) | 日常开发、`drawify lint` 默认 |
| `strict` | `ci` | 仅硬约束 6 条（无交叉、无边框） | CI 门禁、`validate --layout-check` |
| `verbose` | `all` | 全部 8 条 | 调试、排查走廊贴边 |

### 各预设规则开关一览

| 规则 | default | strict | verbose |
|------|---------|--------|---------|
| `node_overlap` | on | on | on |
| `group_overlap` | on | on | on |
| `node_outside_group` | on | on | on |
| `child_group_outside_parent` | on | on | on |
| `edge_through_node` | on | on | on |
| `edge_crosses_group_interior` | on | on | on |
| `edge_crossing` | on (warning) | off | on (warning) |
| `edge_on_group_border` | **off** | off | on (warning) |

---

## CLI 用法

### `drawify lint`

```bash
# 默认预设
drawify lint diagram.dfy

# CI 门禁（仅硬约束）
drawify lint diagram.dfy --profile strict
drawify lint diagram.dfy --profile ci

# 全规则（含贴边框）
drawify lint diagram.dfy --profile verbose

# 忽略指定规则（逗号分隔）
drawify lint diagram.dfy --ignore edge_crossing,edge_on_group_border

# warning 也导致退出码 1
drawify lint diagram.dfy --fail-on-warning

# JSON 输出（便于脚本处理）
drawify lint diagram.dfy --format json
```

退出码：

- `0` — 在当前配置下可接受（`LintReport::is_acceptable`）
- `1` — 存在 error，或开启了 `--fail-on-warning` 且存在 warning

### `drawify validate --layout-check`

验证语法语义通过后，额外运行 **`strict` 预设**的 lint（与 `drawify lint --profile strict` 等价）。

```bash
drawify validate diagram.dfy --layout-check
```

### 文本输出示例

```text
⚠ 布局 lint 发现 2 个 error、18 个 warning:
[error] edge_through_node: 边 api → db 穿过节点 'cache'
  entities: api, db, cache
  edge_index: 3
[warning] edge_crossing: 边 index=0 与边 index=1 交叉
  edge_index: 0
```

### JSON 输出结构

```json
{
  "violations": [
    {
      "rule": "node_overlap",
      "severity": "error",
      "message": "节点 'a' 与 'b' 重叠",
      "metric": 2400.0,
      "entity_ids": ["a", "b"],
      "group_ids": [],
      "edge_index": null
    }
  ]
}
```

| 字段 | 说明 |
|------|------|
| `rule` | 规则 ID（snake_case） |
| `severity` | `error` / `warning` |
| `message` | 人类可读描述 |
| `metric` | 可选量化值（重叠面积 px²、超出距离 px 等） |
| `entity_ids` | 相关实体 id |
| `group_ids` | 相关分组 id |
| `edge_index` | 边在 `diagram.relations` 中的下标 |

---

## Rust API

### 快速入口

```rust
use drawify_core::layout::{compute_layout_with_plan, lint_layout, LayoutLinter, LintConfig};

let layout = compute_layout_with_plan(diagram, layout_plan)?;
let report = lint_layout(diagram, &layout);

if !report.is_clean() {
    for v in &report.violations {
        eprintln!("[{}] {}", v.rule.as_str(), v.message);
    }
}
```

### 自定义配置

```rust
use drawify_core::layout::{
    LayoutLinter, LintConfig, LintProfile, LintRuleId, LintSeverity, RuleConfig,
};

// 预设
let config = LintConfig::profile(LintProfile::Strict);

// 链式定制
let config = LintConfig::strict()
    .without(&[LintRuleId::EdgeCrossesGroupInterior])
    .with_fail_on_warning(false);

// 单条规则覆盖严重级别
let mut config = LintConfig::default();
config.rules[LintRuleId::EdgeThroughNode.index()] =
    RuleConfig::on_with_severity(LintSeverity::Warning);

let report = LayoutLinter::with_config(config.clone()).run(diagram, &layout);

if !report.is_acceptable(&config) {
    // error，或 fail_on_warning 时的 warning
}
```

### 主要类型

#### `LayoutLinter`

```rust
pub struct LayoutLinter {
    pub config: LintConfig,
}

impl LayoutLinter {
    pub fn new() -> Self;
    pub fn with_config(config: LintConfig) -> Self;
    pub fn run(&self, diagram: &Diagram, result: &LayoutResult) -> LintReport;
}
```

#### `LintConfig`

```rust
impl LintConfig {
    pub fn profile(profile: LintProfile) -> Self;
    pub fn default_preset() -> Self;  // = LintProfile::Default
    pub fn strict() -> Self;
    pub fn verbose() -> Self;

    pub fn is_enabled(&self, id: LintRuleId) -> bool;
    pub fn severity_for(&self, id: LintRuleId) -> LintSeverity;

    pub fn without(self, ids: &[LintRuleId]) -> Self;
    pub fn only(self, ids: &[LintRuleId]) -> Self;
    pub fn with_fail_on_warning(self, fail: bool) -> Self;
}
```

#### `LintReport`

```rust
impl LintReport {
    pub fn is_clean(&self) -> bool;           // 无 error
    pub fn is_acceptable(&self, config: &LintConfig) -> bool;
    pub fn error_count(&self) -> usize;
    pub fn warning_count(&self) -> usize;
    pub fn by_rule(&self, rule: LintRuleId) -> impl Iterator<Item = &LayoutViolation>;
}
```

#### `LayoutViolation`

```rust
pub struct LayoutViolation {
    pub rule: LintRuleId,
    pub severity: LintSeverity,
    pub message: String,
    pub metric: Option<f64>,
    pub entity_ids: Vec<String>,
    pub group_ids: Vec<String>,
    pub edge_index: Option<usize>,
}
```

#### 解析辅助（CLI 同款）

```rust
pub fn parse_lint_profile(s: &str) -> Option<LintProfile>;
pub fn parse_lint_rule(s: &str) -> Option<LintRuleId>;
pub fn parse_lint_rules_list(s: &str) -> Vec<LintRuleId>;
```

### 与布局管线的衔接

```rust
use drawify_core::ast::PreparedDiagram;
use drawify_core::layout::{compute_layout_with_plan, LayoutLinter, LintConfig};

fn lint_prepared(prepared: &PreparedDiagram) -> drawify_core::layout::LintReport {
    let diagram = prepared.inner();
    let layout = compute_layout_with_plan(diagram, prepared.layout_plan())
        .expect("layout");
    LayoutLinter::with_config(LintConfig::strict()).run(diagram, &layout)
}
```

`validate_group_containment`（`LayoutResult` 上的旧 API）仍可用；`node_outside_group` 与 `child_group_outside_parent` 规则内部复用同一检测逻辑。

---

## 与 SVG debug 元数据的关系

开启 `svg-debug` feature 时，SVG 会带 `data-dfy-*` 属性（`data-dfy-id`、`data-dfy-index` 等），便于在浏览器 DevTools 里对照 DSL。

LayoutLint **不读取 SVG**；两者互补：

- lint → 机器可读的违规列表 + CI
- `data-dfy-*` → 可视化定位

将来可在违规元素上附加 `data-dfy-lint` 高亮，需另做渲染层集成。

---

## 推荐使用方式

| 场景 | 建议 |
|------|------|
| 本地改布局算法 | `drawify lint foo.dfy`，默认 preset |
| CI / pre-commit | `drawify lint showcase/ --profile strict` 或批量脚本 |
| 验证 .dfy 语法 + 布局 | `drawify validate foo.dfy --layout-check` |
| 排查走廊贴边 | `--profile verbose` 或 `--ignore` 反向排除 |
| 算法回归评分 | 继续用 `drawify-eval`；后续可把 lint 计数纳入 eval |

---

## 相关文档

- [布局模块 readme](../../crates/drawify-core/src/layout/readme.md)
- [drawify-eval readme](../../crates/drawify-eval/readme.md) — 质量评分与算法对比
- [layout-routing-friendliness-evaluation](../已经实现的方案/layout-routing-friendliness-evaluation.md) — 路由友好性研究（eval 指标来源）
