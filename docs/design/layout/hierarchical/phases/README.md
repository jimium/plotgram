# Hierarchical · 相级设计索引

> 父页：[architecture.md](../architecture.md)  
> 状态：目标契约的细化；不记录实现进度

本目录展开 `architecture.md` 已决定的契约。发生冲突时以父页的写权、IR 与失败语义为准；相级文档不得另造第二套 Plan 或修改 Writer 归属。

| 文档 | 回答的问题 |
|------|------------|
| [contracts-and-ir](contracts-and-ir.md) | 跨 crate 输入输出、稳定 key、Stage 外壳、Diagnostics |
| [composition](composition.md) | FAS、ranking、properify、ordering、约束投影与 Plan freeze |
| [ports-and-channel](ports-and-channel.md) | 边侧约束 + FREE、gate/scope、Channel、track、bundle、DeferToRouter |
| [channel-d1](channel-d1.md) | D₁ 分阶段契约（D1.0 TrackOrder → D1.1 Substrate 搜索 → D1.2 Gate/rip-up） |
| [channel-corridor-allocator](channel-corridor-allocator.md) | D1.3 Corridor Allocator 执行方案（span 亲和 → 内层优先 → RouteOrder → Demand → 回边侧别） |
| [coordinate-and-demand](coordinate-and-demand.md) | Demand epoch、BK/VPSC、组框、PartitionGrid、Orientation |
| [symmetry-axis](symmetry-axis.md) | 次轴对称目标函数 J(x)（主链共线 ∩ 扇出对称；P4） |
| [port-lanes](port-lanes.md) | 双胞胎 N/S 走廊端口绝对列（无 grid；对照 yFiles PortAlignment） |
| [ink-and-verification](ink-and-verification.md) | Ink 纯展开、规范化与分相 verifier |

## 共同格式

每篇至少写清：

1. 输入与输出类型；
2. 本相拥有的自由度；
3. 稳定迭代序与 tie-break；
4. 构造不变量；
5. 失败类别；
6. 下游只读哪些字段。

## 禁止

- 用实现文件名替代相契约；
- 在 phase 文档声明 profile / diagram type 分支；
- 以「后续修一下」绕过 Plan/Metric verifier；
- 为 stub 保留兼容字段或双真源。
