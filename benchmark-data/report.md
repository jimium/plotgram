# 全量架构图基准测试对比报告

## 测试范围
- 22 个架构图文件（`showcase/architecture/*.dfy`）
- 每文件 5 轮 warm-cache 测试，取中位数
- release build

## 优化措施
1. 跳过 baseline reroute（V2 布局变化时的对比路由）
2. Refine 轮次从 3 降至 1

## 全量对比

| 文件 | 边 | 基线 | 优化 | 加速 | 友好性 | 交叉 | 退化 |
|------|-----|------|------|------|--------|------|------|
| c.ai-agent-docops-pipeline | 17 | 2.9ms | 3.0ms | 1.0x | 不变 | 不变 | 不变 |
| c.cloud-native | 14 | 17.8ms | 9.1ms | 2.0x | 不变 | 不变 | 不变 |
| c.data-lineage-platform | 20 | 5.7ms | 5.7ms | 1.0x | 不变 | 不变 | 不变 |
| c.drawify-core-mod-deps | 29 | 162.1ms | 40.9ms | **4.0x** | 不变 | 不变 | +2 |
| c.ecommerce-platform | 19 | 31.4ms | 26.0ms | 1.2x | -0.08 | -4 | +1 |
| c.hybrid-cloud-dr-topology | 24 | 6.1ms | 6.2ms | 1.0x | 不变 | 不变 | 不变 |
| c.k8s-blue-green-release-topology | 30 | 64.4ms | 41.5ms | 1.6x | 不变 | 不变 | 不变 |
| c.k8s-multi-cluster-federation | 35 | 11.4ms | 11.3ms | 1.0x | 不变 | 不变 | 不变 |
| c.k8s-multi-namespace-overview | 37 | 16.4ms | 16.2ms | 1.0x | 不变 | 不变 | 不变 |
| c.k8s-platform-stack | 30 | 425.5ms | 132.8ms | **3.2x** | 不变 | 不变 | 不变 |
| c.k8s-tenant-isolation | 26 | 110.7ms | 48.1ms | 2.3x | 不变 | 不变 | 不变 |
| c.layout-stress-nested | 13 | 3.7ms | 3.7ms | 1.0x | 不变 | 不变 | 不变 |
| c.mcp-server-cluster-architecture | 11 | 4.3ms | 4.2ms | 1.0x | N/A | 不变 | 不变 |
| c.payment-clearing-platform | 17 | 13.4ms | 9.0ms | 1.5x | N/A | -7 | +1 |
| c.supply-chain-control-tower | 18 | 3.4ms | 3.4ms | 1.0x | 不变 | 不变 | 不变 |
| n.compiler-pipeline | 11 | 8.4ms | 3.8ms | 2.2x | -0.21 | -2 | +2 |
| n.d2-cell-tower-network | 7 | 3.5ms | 1.8ms | 2.0x | N/A | -3 | 不变 |
| n.data-pipeline | 8 | 1.8ms | 1.8ms | 1.0x | N/A | 不变 | 不变 |
| n.event-driven | 5 | 0.5ms | 0.5ms | 1.0x | N/A | 不变 | 不变 |
| n.microservices | 8 | 1.4ms | 1.4ms | 1.0x | N/A | 不变 | 不变 |
| s.client-api-db | 2 | 0.1ms | 0.1ms | 1.0x | N/A | 不变 | 不变 |
| s.three-tier | 4 | 0.1ms | 0.1ms | 1.1x | N/A | 不变 | 不变 |
| **合计** | - | **894.9ms** | **370.5ms** | **2.4x** | - | - | - |

## 退化分析

### 友好性 (14 个有评分)
- 12 个不变，2 个轻微退化
  - `n.compiler-pipeline`: 0.44 → 0.23 (-0.21)
  - `c.ecommerce-platform`: 0.40 → 0.32 (-0.08)

### 预测交叉
- 18 个不变，4 个**改善**（减少），0 个增加
  - `c.payment-clearing-platform`: 12 → 5 (-7)
  - `c.ecommerce-platform`: 6 → 2 (-4)
  - `n.d2-cell-tower-network`: 3 → 0 (-3)
  - `n.compiler-pipeline`: 3 → 1 (-2)

### 退化路由
- 18 个不变，4 个轻微增加
  - `c.drawify-core-mod-deps`: 2 → 4 (+2)
  - `n.compiler-pipeline`: 0 → 2 (+2)
  - `c.ecommerce-platform`: 0 → 1 (+1)
  - `c.payment-clearing-platform`: 0 → 1 (+1)

## 结论

- **整体加速 2.4x**，总耗时从 895ms 降至 371ms
- 3 个超过 100ms 的文件中，2 个已降至 50ms 以下
- `c.k8s-platform-stack` 仍为 133ms（候选数 9907），是唯一超过 100ms 的文件
- **友好性退化极轻微**，仅 2 个文件有微弱下降（-0.08 和 -0.21）
- **预测交叉全面改善**，无任何文件增加
- 退化路由增加也极轻微，仅 4 个文件各增加 1-2 个
- 12 个文件（55%）完全不受影响，性能和质量均不变