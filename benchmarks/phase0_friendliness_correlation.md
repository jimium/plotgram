# Phase 0 路由友好性预测度量相关性分析报告

样本数: 179（来自 showcase 全量 .dfy × 各图类型适用布局算法）

## 1. 描述性统计

| 度量 | min | median | max | mean | nonzero_count |
|------|-----|--------|-----|------|---------------|
| channel_congestion | 0.00 | 9.42 | 486.93 | 29.53 | 149 / 179 |
| long_edge_count | 0.00 | 0.00 | 10.00 | 0.65 | 37 / 179 |
| group_gap_deficit | 0.00 | 0.00 | 85.15 | 2.82 | 15 / 179 |
| predicted_crossings | 0.00 | 2.00 | 35.00 | 4.80 | 113 / 179 |
| port_conflict_score | 0.00 | 30.00 | 710.00 | 96.90 | 101 / 179 |
| edge_node_crossings (事后) | 0.00 | 0.00 | 18.00 | 1.08 | 61 / 179 |
| edge_crossings (事后) | 0.00 | 0.00 | 103.00 | 5.79 | 87 / 179 |

## 2. Pearson 相关系数

预测度量与事后路由质量度量的 Pearson 相关性：

| 预测度量 | vs edge_node_crossings | vs edge_crossings |
|----------|----------------------|-------------------|
| channel_congestion | 0.0106 | -0.0148 |
| long_edge_count | 0.3181 | -0.0931 |
| group_gap_deficit | 0.1179 | 0.2503 |
| predicted_crossings | 0.4915 | 0.4623 |
| port_conflict_score | 0.2093 | 0.4787 |

## 3. 权重推荐

以 `edge_node_crossings` 相关性为权重依据（设计文档 §5.2.3 验收标准）：

| 度量 | r(edge_node_crossings) | 权重 w |
|------|------------------------|--------|
| channel_congestion | 0.0106 | 0.0092 (w1 (congestion)) |
| long_edge_count | 0.3181 | 0.2772 (w2 (long_edge)) |
| group_gap_deficit | 0.1179 | 0.1027 (w3 (group_gap)) |
| predicted_crossings | 0.4915 | 0.4284 (w4 (predicted_crossings)) |
| port_conflict_score | 0.2093 | 0.1824 (w5 (port_conflict)) |

## 4. 复合友好度分数相关性

各预测度量 z-score 归一化后按推荐权重加权求和，计算复合分数与事后度量的 Pearson：

| 复合分数 vs | Pearson |
|------------|---------|
| edge_node_crossings | 0.5233 |
| edge_crossings | 0.4274 |

## 5. V1 评估器 friendliness_score 相关性（Phase 1 验证）

V1 评估器（`RoutingFriendlinessEvaluator`）输出的复合分数与事后度量的 Pearson：

| V1 friendliness_score vs | Pearson |
|--------------------------|---------|
| edge_node_crossings | 0.5324 |
| edge_crossings | 0.4876 |

## 6. 验收判定

### 单维度量

- 最佳单维 vs `edge_node_crossings` Pearson = 0.4915（验收阈值 > 0.6）→ ⚠️ 接近
- 最佳单维 vs `edge_crossings` Pearson = 0.4787（验收阈值 > 0.5）→ ⚠️ 接近

### 复合分数（五维加权）

- 复合 vs `edge_node_crossings` Pearson = 0.5233（验收阈值 > 0.6）→ ⚠️ 接近
- 复合 vs `edge_crossings` Pearson = 0.4274（验收阈值 > 0.5）→ ⚠️ 接近

### V1 评估器 friendliness_score

- V1 vs `edge_node_crossings` Pearson = 0.5324（验收阈值 > 0.6）→ ⚠️ 接近
- V1 vs `edge_crossings` Pearson = 0.4876（验收阈值 > 0.5）→ ⚠️ 接近

## 7. 样本明细（按 edge_node_crossings 降序，前 20）

| diagram | layout | enc | ec | cong | long | gap | pred | port | v1_score |
|---------|--------|-----|----|------|------|-----|------|------|----------|
| c.layout-stress-dense | sugiyama | 18 | 0 | 4.23 | 0 | 0 | 18 | 200 | 0.6542 |
| c.layout-stress-dense | sugiyama-v2 | 15 | 6 | 10.94 | 10 | 0 | 15 | 200 | 0.9100 |
| c.layout-stress-dense | er | 14 | 8 | 11.80 | 10 | 0 | 14 | 200 | 0.9100 |
| c.drawify-core-mod-deps | force-directed | 9 | 23 | 6.40 | 0 | 0 | 22 | 250 | 0.6575 |
| c.k8s-multi-namespace-overview | architecture | 9 | 49 | 32.35 | 0 | 0 | 15 | 230 | 0.6600 |
| c.k8s-platform-stack | force-directed | 9 | 48 | 15.70 | 0 | 41 | 8 | 430 | 0.7330 |
| c.caffe-shop | force-directed | 8 | 32 | 34.86 | 0 | 0 | 27 | 190 | 0.6596 |
| c.k8s-multi-cluster-federation | force-directed | 7 | 86 | 18.81 | 0 | 62 | 35 | 360 | 0.7500 |
| c.k8s-platform-stack | architecture | 5 | 63 | 55.19 | 0 | 0 | 20 | 360 | 0.6600 |
| c.k8s-blue-green-release-topology | force-directed | 4 | 32 | 12.92 | 0 | 1 | 18 | 320 | 0.6622 |
| c.k8s-tenant-isolation | force-directed | 4 | 21 | 14.56 | 0 | 52 | 10 | 190 | 0.7500 |
| n.social-network | sugiyama | 4 | 0 | 45.50 | 0 | 0 | 4 | 80 | 0.3660 |
| c.k8s-incident-response | flowchart | 4 | 5 | 21.21 | 5 | 0 | 11 | 30 | 0.7420 |
| c.k8s-incident-response | sugiyama-v2 | 4 | 5 | 21.21 | 5 | 0 | 11 | 30 | 0.7420 |
| c.k8s-node-pressure-lifecycle | circular | 4 | 10 | 19.59 | 0 | 0 | 8 | 116 | 0.6190 |
| c.payment-flow | state | 4 | 1 | 13.45 | 0 | 0 | 4 | 96 | 0.5274 |
| c.service-degradation-lifecycle | circular | 4 | 5 | 31.19 | 0 | 0 | 5 | 8 | 0.4392 |
| c.ai-agent-docops-pipeline | architecture | 3 | 26 | 60.28 | 0 | 0 | 9 | 60 | 0.5640 |
| c.drawify-core-mod-deps | architecture | 3 | 26 | 57.12 | 0 | 16 | 17 | 640 | 0.6888 |
| c.k8s-incident-response | sugiyama | 3 | 0 | 14.40 | 0 | 0 | 12 | 0 | 0.4200 |
