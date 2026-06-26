# c.k8s-tenant-isolation.dfy 基准测试数据

## 图信息
- 文件: showcase/architecture/c.k8s-tenant-isolation.dfy
- 节点: 18 | 边: 26 | 分组: 2

## 原始耗时 (release build, 10 轮)

| 阶段 | 耗时 |
|------|------|
| 解析+prepare | ~0.8ms |
| 布局 (compute_with_overlay) | ~0.3ms |
| Pre-route | ~0.1ms |
| router.route (per-edge + fix_inversions) | ~12ms |
| run_refine (3 passes) | ~51ms |
| Baseline reroute | ~30ms |
| Post-process | ~10ms |
| **总耗时 (中位数)** | **~110ms** |

## 瓶颈分析

1. **run_refine (51ms, 46%)**: 边缘精炼，3轮迭代，每轮分析交叉+推边+重路由
2. **Baseline reroute (30ms, 27%)**: V2布局变化时触发第二套路由+精炼
3. **Post-process (10ms, 9%)**: 网格吸附 + 边排斥
4. **router.route (12ms, 11%)**: 正交路由本身
5. **其他 (<2ms, 2%)**: 布局、解析

## 优化方向

- 跳过 baseline reroute (节省 ~30ms)
- 减少 refine 轮次到 1 (节省 ~34ms)
- 或禁用 refine (节省 ~51ms，但会增加边-节点交叉)