# drawify-eval — Drawify 布局与边路由算法评估框架

提供完整的算法评估能力，用于开发和改进布局/路由算法时量化质量、对比差异、追踪回归。

## 模块结构

```
drawify-eval
├── metrics   — 布局质量指标计算（正确性/紧凑性/均匀性/美观性）
├── profile   — 图结构特征分析（拓扑标签/规模分桶/密度/环检测）
├── engine    — 核心评估引擎（单算法评估/多算法对比/差异报告/组合评估）
├── report    — 报告输出（Markdown/JSON，含排名/推荐/共性问题/规模统计）
└── history   — 结果持久化与历史追踪（保存/加载/基线对比）
```

## 快速开始

```rust
use drawify_eval::engine::{EvalEngine, presets};
use drawify_eval::report::EvalReport;
use drawify_core::pipeline;

// 1. 解析图
let diagram = pipeline::parse(source).unwrap();

// 2. 创建引擎（默认权重）
let engine = EvalEngine::new();

// 3. 评估单个算法
let config = presets::set_layout_algo("sugiyama");
let result = engine.evaluate(&diagram, &config);
println!("评分: {:.1} ({})", result.score, result.quality_grade);

// 4. 对比多个算法
let configs = presets::routing_comparison();
let report = engine.compare("my_diagram", &diagram, &configs);
println!("{}", report.to_markdown());
```

---

## 核心 API

### EvalEngine — 评估引擎

```rust
// 创建引擎的三种方式
let engine = EvalEngine::new();                                          // 默认权重
let engine = EvalEngine::with_weights_for_type(&DiagramType::Flowchart); // 按图类型权重
let engine = EvalEngine::with_weights(MetricWeights {                    // 自定义权重
    correctness: 0.40, compactness: 0.20,
    uniformness: 0.20, aesthetics: 0.20,
});
```

| 方法 | 用途 |
|------|------|
| `evaluate(&diagram, &config)` | 评估单个算法，返回 `EvalResult` |
| `compare(name, &diagram, &configs)` | 对比多个算法，返回 `ComparisonReport`（含排名+推荐） |
| `diff(&baseline, &current)` | 生成差异报告，检测回归和改善 |
| `evaluate_combinations(name, &diagram)` | 穷举布局+路由组合，返回 `CombinationReport` |
| `find_worst_cases(&results, algo, top_n)` | 找到算法表现最差的案例 |
| `evaluate_layout(&diagram, algo, layout, routing, &result)` | 从已有 LayoutResult 创建评估结果 |

### AlgorithmConfig — 算法配置

```rust
// 使用 presets 快速创建
let config = presets::set_layout_algo("sugiyama");
let config = presets::set_edge_routing("orthogonal");
let config = presets::set_layout_and_routing("sugiyama", "orthogonal");
```

### 预定义配置组

| 函数 | 说明 |
|------|------|
| `presets::layout_comparison()` | 4 种常用布局算法对比 |
| `presets::full_layout_comparison()` | 全部 11 种布局算法对比 |
| `presets::routing_comparison()` | 6 种边路由对比 |
| `presets::sugiyama_routing_comparison()` | sugiyama + 6 种路由 |
| `presets::layout_algos_for_type(&dtype)` | 按图类型获取适用布局 |
| `presets::routing_algos_for_type(&dtype)` | 按图类型获取适用路由 |

---

## 指标体系

### LayoutMetrics — 四维质量指标

| 维度 | 指标 | 方向 | 说明 |
|------|------|------|------|
| **正确性** | `node_overlap_pairs` | 越低越好 | 节点重叠对数 |
| | `edge_node_crossings` | 越低越好 | 边穿过非起终节点数 |
| | `edge_crossings` | 越低越好 | 边交叉数 |
| **紧凑性** | `total_area` | 越低越好 | 画布面积 |
| | `total_edge_length` | 越低越好 | 边总长 |
| | `area_utilization` | 越高越好 | 面积利用率 |
| **均匀性** | `edge_length_cv` | 越低越好 | 边长变异系数 |
| **美观性** | `aspect_ratio` | 接近 1.6 最佳 | 宽高比 |

### 综合评分

默认权重：正确性 40% + 紧凑性 20% + 均匀性 20% + 美观性 20%

```rust
let metrics = LayoutMetrics::compute(&diagram, &layout);
let score = metrics.quality_score();                        // 默认权重
let score = metrics.quality_score_with_weights(&weights);   // 自定义权重
let dims = metrics.dimension_scores();                      // 各维度得分明细
```

### MetricWeights — 按图类型定制权重

| 图类型 | 正确性 | 紧凑性 | 均匀性 | 美观性 |
|--------|--------|--------|--------|--------|
| Flowchart | 0.40 | 0.15 | 0.20 | 0.25 |
| Architecture | 0.30 | 0.25 | 0.20 | 0.25 |
| State | 0.40 | 0.15 | 0.25 | 0.20 |
| ER | 0.35 | 0.20 | 0.25 | 0.20 |
| Sequence | 0.50 | 0.20 | 0.15 | 0.15 |
| Mindmap | 0.25 | 0.15 | 0.30 | 0.30 |

### QualityGrade — 质量等级

| 等级 | 分数范围 | Display |
|------|----------|---------|
| Excellent | >= 85 | 优秀 |
| Good | >= 70 | 良好 |
| Acceptable | >= 50 | 可接受 |
| Poor | < 50 | 较差 |

---

## 图结构特征分析

### GraphProfile

```rust
let profile = GraphProfile::analyze(&diagram);
```

| 字段 | 说明 |
|------|------|
| `node_count` / `edge_count` | 节点/边数量 |
| `density` | 图密度（实际边数/最大可能边数） |
| `max_depth` | 最长路径深度 |
| `max_fan_out` / `max_fan_in` | 最大出度/入度 |
| `has_cycles` | 是否存在环 |
| `avg_branching` | 平均分支因子 |
| `size_bucket` | 规模分桶 |
| `topology_tags` | 拓扑标签列表 |

### SizeBucket — 规模分桶

| 分桶 | 节点数范围 |
|------|-----------|
| Tiny | <= 5 |
| Small | 6-15 |
| Medium | 16-40 |
| Large | 41-100 |
| Huge | > 100 |

### TopologyTag — 拓扑标签

| 标签 | 判定条件 |
|------|----------|
| Chain | 深度 > 5，最大出度 <= 2 |
| WideFanOut | 最大出度 >= 4 |
| Dense | 密度 > 0.3 |
| Sparse | 密度 < 0.1 |
| Cyclic | 存在环 |
| Tree | 无环且边数 = 节点数 - 1 |
| Hub | 存在度 >= 5 的节点 |

---

## 差异报告与回归检测

```rust
let baseline = engine.evaluate(&diagram, &config_a);
let current = engine.evaluate(&diagram, &config_b);

let diff = engine.diff(&baseline, &current);

// 评分变化（正值=改善，负值=退步）
println!("评分变化: {:+.1}", diff.score_diff);

// 回归检测
for reg in &diff.regressions {
    println!("回归: {} ({:?})", reg.metric, reg.severity);
}

// 改善
for imp in &diff.improvements {
    println!("改善: {} {:.2} -> {:.2}", imp.metric, imp.baseline, imp.current);
}
```

### RegressionSeverity

| 等级 | 百分比退步 |
|------|-----------|
| Minor | < 5% |
| Moderate | 5-15% |
| Major | > 15% |

---

## 结果持久化与历史追踪

```rust
use drawify_eval::history::HistoryStore;

let store = HistoryStore::new(Path::new(".eval-history"))?;

// 保存
store.save(&report, "baseline")?;

// 加载最新
let latest = store.load_latest()?;

// 按标签加载
let baseline = store.load("baseline")?;

// 列出所有历史
for entry in store.list()? {
    println!("{} - {:?}", entry.label, entry.modified);
}

// 与最新历史对比
if let Some(diffs) = store.compare_with_latest(&current_report, &engine) {
    for diff in &diffs {
        if !diff.regressions.is_empty() {
            println!("检测到回归！");
        }
    }
}
```

---

## 报告输出

### EvalReport

```rust
let mut report = EvalReport::new("批量评估报告");
report.add_comparison(comp_report);
report.add_diff(diff_report);
report.add_combination(combo_report);

// 输出格式
let markdown = report.to_markdown();  // Markdown（含排名表/推荐/共性问题/规模统计）
let json = report.to_json();          // JSON

// 写入文件（根据扩展名自动选格式）
report.write_to_file(Path::new("report.md"))?;
report.write_to_file(Path::new("report.json"))?;
```

### Markdown 报告包含

1. 各图的对比表格（含图特征摘要、排名、推荐、详细指标）
2. 布局+路由组合评估矩阵
3. 差异报告（回归/改善/指标明细）
4. 按图表类型分组排名
5. 共性问题分析（所有算法评分均低的图）
6. 按规模分桶统计

---

## CLI 使用

```bash
# 评估单个文件
drawify-eval eval flowchart.dfy

# 指定算法评估
drawify-eval eval flowchart.dfy -a sugiyama

# 布局+路由组合评估
drawify-eval eval flowchart.dfy -c combinations

# 批量评估目录
drawify-eval batch showcase/

# 按图类型权重评估
drawify-eval batch showcase/ -w per-type

# 与基线对比
drawify-eval batch showcase/ --baseline .eval-history/baseline.json

# 保存历史
drawify-eval batch showcase/ --save-history

# 算法维度评估（指定算法，自动找适用图）
drawify-eval algo sugiyama showcase/

# 对比两次评估结果 JSON
drawify-eval diff baseline.json current.json
```

---

## 典型工作流

### 1. 开发新算法时 — 单算法快速验证

```rust
let engine = EvalEngine::with_weights_for_type(&DiagramType::Flowchart);
let config = presets::set_layout_algo("my-new-algo");
let result = engine.evaluate(&diagram, &config);

if result.quality_grade == QualityGrade::Poor {
    let dims = result.metrics.dimension_scores();
    println!("最弱维度: correctness={:.2} compactness={:.2}", dims.correctness, dims.compactness);
}
```

### 2. 改进算法后 — 回归检测

```rust
let store = HistoryStore::new(Path::new(".eval-history"))?;
let current_report = /* 当前评估结果 */;

if let Some(diffs) = store.compare_with_latest(&current_report, &engine) {
    for diff in &diffs {
        if diff.regressions.iter().any(|r| r.severity == RegressionSeverity::Major) {
            eprintln!("严重回归！请检查算法变更");
        }
    }
}

store.save(&current_report, "after-optimization")?;
```

### 3. 选择最佳算法 — 组合评估

```rust
let engine = EvalEngine::with_weights_for_type(&diagram.diagram_type);
let combo = engine.evaluate_combinations("my_graph", &diagram);

if let Some(best) = &combo.best_combination {
    println!("最佳组合: {} — {}", best.algorithm, best.reason);
}
```

### 4. 算法横向对比 — 多图批量评估

```rust
let engine = EvalEngine::new();
let mut report = EvalReport::new("算法横向对比");

for diagram in all_diagrams {
    let configs = presets::layout_algos_for_type(&diagram.diagram_type);
    let comp = engine.compare(&diagram_name, &diagram, &configs);
    report.add_comparison(comp);
}

// 报告自动包含按类型排名、共性问题、规模统计
let md = report.to_markdown();
```

### 5. 发现算法弱点 — 最差案例

```rust
let all_results: Vec<EvalResult> = /* 收集所有评估结果 */;
let worst = engine.find_worst_cases(&all_results, "sugiyama", 5);

for r in &worst {
    println!("{}: {:.1}分 ({:?}) — {}",
        r.graph_profile.topology_summary(),
        r.score, r.quality_grade,
        r.graph_profile.size_bucket
    );
}
```

---

## 关键类型速查

| 类型 | 模块 | 说明 |
|------|------|------|
| `EvalEngine` | engine | 评估引擎 |
| `AlgorithmConfig` | engine | 算法配置 |
| `EvalResult` | engine | 单算法评估结果 |
| `ComparisonReport` | engine | 多算法对比报告 |
| `DiffReport` | engine | 差异报告 |
| `Regression` | engine | 回归 |
| `Improvement` | engine | 改善 |
| `CombinationReport` | engine | 组合评估报告 |
| `RankingEntry` | engine | 排名条目 |
| `Recommendation` | engine | 推荐 |
| `LayoutMetrics` | metrics | 布局质量指标 |
| `MetricWeights` | metrics | 指标权重 |
| `QualityGrade` | metrics | 质量等级 |
| `DimensionScores` | metrics | 维度得分 |
| `GraphProfile` | profile | 图结构特征 |
| `SizeBucket` | profile | 规模分桶 |
| `TopologyTag` | profile | 拓扑标签 |
| `EvalReport` | report | 批量评估报告 |
| `HistoryStore` | history | 历史存储 |
| `HistoryEntry` | history | 历史条目 |

所有数据类型均实现 `serde::Serialize + serde::Deserialize`，可直接序列化为 JSON。
