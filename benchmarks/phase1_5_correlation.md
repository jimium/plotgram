# Phase 1.5 路由友好性预测度量相关性分析报告

样本数: 792

## 1. 全局描述性统计

| 度量 | min | median | max | mean | nonzero_count |
|------|-----|--------|-----|------|---------------|
| channel_congestion | 1.00 | 9.00 | 54.00 | 10.97 | 792 / 792 |
| long_edge_count | 0.00 | 0.00 | 47.00 | 1.12 | 116 / 792 |
| group_gap_deficit | 0.00 | 0.00 | 85.15 | 0.66 | 17 / 792 |
| predicted_crossings | 0.00 | 1.00 | 70.00 | 5.35 | 410 / 792 |
| port_conflict_score | 0.00 | 30.00 | 2040.00 | 143.32 | 424 / 792 |
| edge_node_crossings (事后) | 0.00 | 0.00 | 121.00 | 1.91 | 246 / 792 |
| edge_crossings (事后) | 0.00 | 1.00 | 152.00 | 8.06 | 433 / 792 |

## 2. 全局 Pearson + Spearman 相关系数

| 预测度量 | Pearson vs enc | Pearson vs ec | Spearman vs enc | Spearman vs ec |
|----------|----------------|---------------|-----------------|----------------|
| channel_congestion | 0.2315 | 0.7434 | 0.3654 | 0.7440 |
| long_edge_count | 0.1584 | 0.1586 | 0.2730 | -0.0331 |
| group_gap_deficit | 0.0066 | 0.0908 | 0.0640 | 0.0994 |
| predicted_crossings | 0.5642 | 0.5631 | 0.6271 | 0.5135 |
| port_conflict_score | 0.2472 | 0.7496 | 0.3248 | 0.6347 |

## 3. 全局权重推荐（Pearson 归一化）

以 `edge_node_crossings` 的 Pearson 相关性为权重依据：

| 度量 | r(enc) | 权重 w |
|------|--------|--------|
| channel_congestion | 0.2315 | 0.1916 (w1 (congestion)) |
| long_edge_count | 0.1584 | 0.1311 (w2 (long_edge)) |
| group_gap_deficit | 0.0066 | 0.0055 (w3 (group_gap)) |
| predicted_crossings | 0.5642 | 0.4671 (w4 (predicted_crossings)) |
| port_conflict_score | 0.2472 | 0.2047 (w5 (port_conflict)) |

## 4. 复合友好度分数相关性（全局）

各预测度量 z-score 归一化后按推荐权重加权求和：

| 复合分数 vs | Pearson | Spearman |
|------------|---------|----------|
| edge_node_crossings | 0.5092 | 0.5839 |
| edge_crossings | 0.7788 | 0.7284 |

## 5. V1 评估器 friendliness_score 相关性（Phase 1.5 分族权重）

V1 评估器（`RoutingFriendlinessEvaluator`，Phase 1.5 分族权重）复合分数：

| V1 friendliness_score vs | Pearson | Spearman |
|--------------------------|---------|----------|
| edge_node_crossings | 0.5515 | 0.5907 |
| edge_crossings | 0.6076 | 0.6194 |

## 5b. V1 z-score 变体（分族 z-score + 分族权重，模拟最优归一化）

| 变体 | Pearson vs enc | Spearman vs enc | Pearson vs ec |
|------|----------------|-----------------|---------------|
| A: 全局 z-score + 分族权重 | 0.5521 | 0.5968 | 0.5870 |
| B: 分族 z-score + 分族权重 | 0.5682 | 0.5947 | 0.6152 |

分族 μ / σ（供硬编码）：

层次类 (n=549):
  congestion μ=10.8962 σ=10.0290
  long_edge μ=1.6157 σ=5.5571
  group_gap μ=0.1749 σ=2.9319
  predicted μ=5.9472 σ=9.9090
  port μ=147.7213 σ=292.1558
力导向类 (n=169):
  congestion μ=11.4675 σ=8.3920
  long_edge μ=0.0000 σ=0.0000
  group_gap μ=1.9479 σ=10.4105
  predicted μ=3.9467 σ=8.3022
  port μ=147.8225 σ=257.1430
放射/分组类 (n=74):
  congestion μ=10.4189 σ=7.3061
  long_edge μ=0.0000 σ=0.0000
  group_gap μ=1.2973 σ=6.1726
  predicted μ=4.1486 σ=6.0687
  port μ=100.4189 σ=138.0807

## 6. 分族相关性分析（Phase 1.5 分组校准）

### 层次类（样本数 549）

| 预测度量 | Pearson vs enc | Spearman vs enc | Pearson vs ec |
|----------|----------------|-----------------|---------------|
| channel_congestion | 0.0544 | 0.2850 | 0.7125 |
| long_edge_count | 0.2449 | 0.3243 | 0.1977 |
| group_gap_deficit | -0.0109 | 0.0513 | -0.0287 |
| predicted_crossings | 0.6244 | 0.5820 | 0.4740 |
| port_conflict_score | 0.0215 | 0.2471 | 0.7522 |

推荐权重: 
- w1 = 0.0576（channel_congestion）
- w2 = 0.2591（long_edge_count）
- w3 = 0.0000（group_gap_deficit）
- w4 = 0.6605（predicted_crossings）
- w5 = 0.0228（port_conflict_score）

V1 评估器（分族权重）vs enc: Pearson = 0.5392, Spearman = 0.5811

### 力导向类（样本数 169）

| 预测度量 | Pearson vs enc | Spearman vs enc | Pearson vs ec |
|----------|----------------|-----------------|---------------|
| channel_congestion | 0.5971 | 0.6579 | 0.8375 |
| long_edge_count | 0.0000 | 0.0000 | 0.0000 |
| group_gap_deficit | 0.0143 | 0.1038 | 0.2120 |
| predicted_crossings | 0.6136 | 0.7787 | 0.8786 |
| port_conflict_score | 0.6884 | 0.6253 | 0.7693 |

推荐权重: 
- w1 = 0.3120（channel_congestion）
- w2 = 0.0000（long_edge_count）
- w3 = 0.0075（group_gap_deficit）
- w4 = 0.3207（predicted_crossings）
- w5 = 0.3598（port_conflict_score）

V1 评估器（分族权重）vs enc: Pearson = 0.6594, Spearman = 0.6728

### 放射/分组类（样本数 74）

| 预测度量 | Pearson vs enc | Spearman vs enc | Pearson vs ec |
|----------|----------------|-----------------|---------------|
| channel_congestion | 0.6591 | 0.4693 | 0.8802 |
| long_edge_count | 0.0000 | 0.0000 | 0.0000 |
| group_gap_deficit | -0.0226 | 0.1492 | 0.1021 |
| predicted_crossings | 0.6898 | 0.6238 | 0.7044 |
| port_conflict_score | 0.6182 | 0.3749 | 0.7884 |

推荐权重: 
- w1 = 0.3351（channel_congestion）
- w2 = 0.0000（long_edge_count）
- w3 = 0.0000（group_gap_deficit）
- w4 = 0.3507（predicted_crossings）
- w5 = 0.3143（port_conflict_score）

V1 评估器（分族权重）vs enc: Pearson = 0.7296, Spearman = 0.5089


## 7. 验收判定

### 样本规模

- 总样本数: 792（目标 ≥ 500）→ ✅ 达标

### 单维度量

- 最佳单维 vs `edge_node_crossings` Pearson = 0.5642（阈值 > 0.6）→ ⚠️ 接近
- 最佳单维 vs `edge_crossings` Pearson = 0.7496（阈值 > 0.5）→ ✅ 通过

### 复合分数（五维加权）

- 复合 vs `edge_node_crossings` Pearson = 0.5092（阈值 > 0.6）→ ⚠️ 接近
- 复合 vs `edge_crossings` Pearson = 0.7788（阈值 > 0.5）→ ✅ 通过

### V1 评估器 friendliness_score（分族权重）

- V1 vs `edge_node_crossings` Pearson = 0.5515（阈值 > 0.6）→ ⚠️ 接近
- V1 vs `edge_crossings` Pearson = 0.6076（阈值 > 0.5）→ ✅ 通过

## 8. 样本明细（按 edge_node_crossings 降序，前 20）

| diagram | layout | family | enc | ec | cong | long | gap | pred | port | v1_score |
|---------|--------|--------|-----|----|------|------|-----|------|------|----------|
| wide-L3P5 | force-directed | 力导向类 | 121 | 121 | 50.00 | 0 | 0 | 42 | 1690 | 5.0478 |
| er-schema2-n10 | sugiyama | 层次类 | 46 | 0 | 10.00 | 0 | 0 | 46 | 80 | 2.5828 |
| er-schema2-n9 | sugiyama | 层次类 | 38 | 0 | 10.00 | 0 | 0 | 38 | 80 | 2.0500 |
| wide-L3P4 | force-directed | 力导向类 | 33 | 53 | 32.00 | 0 | 0 | 18 | 810 | 2.2164 |
| bipartite-5x6 | force-directed | 力导向类 | 30 | 49 | 29.00 | 0 | 0 | 19 | 656 | 2.3197 |
| er-schema2-n10 | er | 层次类 | 30 | 0 | 6.00 | 11 | 0 | 30 | 80 | 2.0076 |
| er-schema2-n10 | sugiyama-v2 | 层次类 | 30 | 0 | 6.00 | 11 | 0 | 30 | 80 | 2.0076 |
| er-schema-n8 | sugiyama | 层次类 | 28 | 0 | 8.00 | 0 | 0 | 28 | 80 | 1.3720 |
| er-schema2-n9 | er | 层次类 | 26 | 0 | 6.00 | 10 | 0 | 26 | 80 | 1.6945 |
| er-schema2-n9 | sugiyama-v2 | 层次类 | 26 | 0 | 6.00 | 10 | 0 | 26 | 80 | 1.6945 |
| dense-n26 | sugiyama | 层次类 | 22 | 88 | 6.00 | 0 | 0 | 70 | 0 | 4.1517 |
| er-schema-n8 | er | 层次类 | 22 | 0 | 6.00 | 9 | 0 | 22 | 80 | 1.3813 |
| er-schema-n8 | sugiyama-v2 | 层次类 | 22 | 0 | 6.00 | 9 | 0 | 22 | 80 | 1.3813 |
| er-schema2-n7 | sugiyama | 层次类 | 22 | 0 | 8.00 | 0 | 0 | 22 | 80 | 0.9724 |
| bipartite-5x6 | sugiyama | 层次类 | 20 | 20 | 45.00 | 0 | 0 | 12 | 840 | 0.5798 |
| c.layout-stress-dense | sugiyama | 层次类 | 18 | 0 | 13.00 | 0 | 0 | 18 | 200 | 0.7441 |
| bipartite-6x6 | force-directed | 力导向类 | 18 | 41 | 36.00 | 0 | 0 | 34 | 1096 | 3.1312 |
| dense-n22 | sugiyama | 层次类 | 18 | 71 | 6.00 | 0 | 0 | 58 | 0 | 3.3525 |
| er-schema2-n7 | er | 层次类 | 18 | 0 | 6.00 | 8 | 0 | 18 | 80 | 1.0682 |
| er-schema2-n7 | sugiyama-v2 | 层次类 | 18 | 0 | 6.00 | 8 | 0 | 18 | 80 | 1.0682 |
