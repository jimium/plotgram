# drawify-eval 使用指南

`drawify-eval` 对布局与边路由算法做**量化评分、A/B 对比和回归检测**。与 LayoutLint 互补：eval 回答「好不好」，lint 回答「哪里错了」。

> 实现：`crates/drawify-eval/`  
> Crate 内详细 API 表：[crates/drawify-eval/readme.md](../../crates/drawify-eval/readme.md)

**注意**：当前 **没有独立的 `drawify-eval` CLI 二进制**，需通过 **Rust API** 或自建脚本调用。下文示例均为库用法。

---

## 与 LayoutLint 的分工

| | LayoutLint | drawify-eval |
|--|------------|--------------|
| 输出 | 逐条 `LayoutViolation` | `LayoutMetrics` 分数 + 等级 |
| 粒度 | 可定位 entity / edge | 图级聚合指标 |
| 典型用途 | CI 硬门禁、调试单图 | 算法对比、回归、选参 |
| 文档 | [layout-lint.md](layout-lint.md) | 本文 |

后续可将 lint 违规计数纳入 eval 的「正确性」维度。

---

## 快速开始

```rust
use drawify_core::pipeline::parse;
use drawify_eval::engine::{EvalEngine, presets};

let raw = parse(source)?;
let diagram = raw.into_inner();

let engine = EvalEngine::new();
let config = presets::set_layout_algo("sugiyama");
let result = engine.evaluate(&diagram, &config);

println!("评分: {:.1} ({})", result.score, result.quality_grade);
println!("节点重叠: {}", result.metrics.node_overlap_pairs);
println!("边交叉: {}", result.metrics.edge_crossings);
```

`evaluate` 内部会：按 `AlgorithmConfig` 跑布局 → 计算 `LayoutMetrics` → 加权得分。

---

## 核心类型

### `EvalEngine`

```rust
let engine = EvalEngine::new();
let engine = EvalEngine::with_weights_for_type(&DiagramType::Flowchart);
let engine = EvalEngine::with_weights(custom_weights);
```

| 方法 | 用途 |
|------|------|
| `evaluate(&diagram, &config)` | 单算法评估 |
| `compare(name, &diagram, &configs)` | 多算法对比 + 排名 |
| `diff(&baseline, &current)` | 两次评估差异、回归检测 |
| `evaluate_combinations(name, &diagram)` | 布局×路由组合穷举 |
| `find_worst_cases(&results, algo, n)` | 找算法最差样例 |
| `evaluate_layout(...)` | 已有 `LayoutResult` 时直接评分 |

### `AlgorithmConfig` 预设

```rust
use drawify_eval::engine::presets;

presets::set_layout_algo("sugiyama");
presets::set_edge_routing("orthogonal");
presets::set_layout_and_routing("sugiyama", "orthogonal");

presets::layout_comparison();           // 常用布局对比组
presets::routing_comparison();          // 路由对比组
presets::layout_algos_for_type(&dtype); // 按图类型筛选
```

### `LayoutMetrics` 维度

| 维度 | 代表指标 | 方向 |
|------|----------|------|
| 正确性 | `node_overlap_pairs`、`edge_node_crossings` | 越低越好 |
| 可读性 | `edge_crossings` | 越低越好 |
| 紧凑性 | `total_area`、`total_edge_length` | 越低越好 |
| 均匀性 | `edge_length_cv` | 越低越好 |
| 美观性 | `aspect_ratio` | 接近黄金比更佳 |

默认权重：正确性 40% + 紧凑性 20% + 均匀性 20% + 美观性 20%（可按 `DiagramType` 调整）。

### `QualityGrade`

| 等级 | 分数 |
|------|------|
| 优秀 | ≥ 85 |
| 良好 | ≥ 70 |
| 可接受 | ≥ 50 |
| 较差 | < 50 |

---

## 多算法对比

```rust
let configs = presets::routing_comparison();
let report = engine.compare("my_diagram", &diagram, &configs);
println!("{}", report.to_markdown());
```

`ComparisonReport` 含排名、推荐算法、各配置明细指标。

---

## 回归检测

```rust
use drawify_eval::history::HistoryStore;

let baseline = engine.evaluate(&diagram, &config_v1);
let current = engine.evaluate(&diagram, &config_v2);
let diff = engine.diff(&baseline, &current);

for reg in &diff.regressions {
    println!("回归: {:?} {:?}", reg.metric, reg.severity);
}

// 持久化
let store = HistoryStore::new(path(".eval-history"))?;
store.save(&report, "after-optimization")?;
```

---

## 批量评估 Showcase

```rust
use drawify_eval::report::EvalReport;

let mut report = EvalReport::new("showcase 批量评估");
for (name, source) in diagrams {
    let raw = parse(&source)?;
    let comp = engine.compare(name, &raw.into_inner(), &configs);
    report.add_comparison(comp);
}
std::fs::write("report.md", report.to_markdown())?;
```

配合 [showcase-workflow.md](showcase-workflow.md) 中的样例集。

---

## 自建 CLI 脚本（示例）

在 `examples/` 或测试二进制中：

```bash
cargo test -p drawify-eval -- --nocapture   # 跑 crate 内测试
# 或编写 bins/eval-cli.rs 调用 EvalEngine
```

若需要一等 CLI，可在 `drawify-eval/Cargo.toml` 增加 `[[bin]]` 并封装 readme 中的命令表。

---

## 相关文档

- [layout-lint.md](layout-lint.md) — 静态违规检查
- [showcase-workflow.md](showcase-workflow.md) — 样例集回归
- [layout-routing-friendliness-evaluation](../已经实现的方案/layout-routing-friendliness-evaluation.md) — 指标设计背景
