# ADR-0000: <决策标题>

- Status: Proposed
- Date: YYYY-MM-DD

## Status 取值说明（必读）

ADR 的 `Status` 建议使用以下枚举：

- `Proposed`：提案中，尚未最终接受。
- `Accepted`：已采纳，作为当前有效决策。
- `Superseded`：该 ADR 已被新的 ADR 替代。
- `Deprecated`：不再推荐，但可能因兼容性暂时保留。

维护建议：

- 状态变更时，更新本文档头部 `Status` 字段；
- 若状态为 `Superseded`，建议在 References 中增加替代 ADR 引用（如 `Superseded-by: ADR-0009`）。

## Context

描述当前问题、背景与约束：

- 我们要解决什么问题？
- 现有实现/现状是什么？
- 约束条件是什么（安全、性能、兼容性、可维护性、依赖限制等）？
- 还有哪些备选方案？

## Decision

一句话给出最终决策，然后用要点列出关键点：

- 选择的方案是什么？
- 核心设计点（接口、数据结构、流程边界）是什么？
- 兼容性与迁移策略是什么？

## Alternatives Considered

列出备选方案与未选原因：

- 方案 A：为什么不选（风险/成本/不满足约束）
- 方案 B：为什么不选

## Consequences

说明带来的影响（好与坏都写）：

- 正向收益
- 代价与新增复杂度
- 风险点与缓解策略
- 后续工作（TODO 列表）

## References

- 相关 PR/Issue/讨论链接（如有）
- 相关规范：`specs/...`