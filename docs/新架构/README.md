# 新架构

布局与路由下一代架构（**Atlas**）的设计与推进文档。

> **当前状态**：设计期，门禁全关（[`AGENTS.md`](../../AGENTS.md) §10）。  
> 开关在 [`benchmarks/scripts/gate-switch.sh`](../../benchmarks/scripts/gate-switch.sh)；临时校验用 `PLOTGRAM_GATES=on`。

## 阅读顺序

| # | 文档 | 作用 |
|---|------|------|
| 21 | [Hierarchical 统一内核与泳道语义](21-Hierarchical统一内核与泳道语义-可行性与演进建议-2026-07.md) | **立场**：要什么、不要什么。group 一等公民、flowchart/arch 去双轨、四布局内核、双路由；DSL swimlane/table 非第一需求 |
| 22 | [Atlas 总纲](22-Atlas下一代布局与路由架构-总纲-2026-07.md) | **设计**：五病灶诊断、B1–B6 公理、术语表、三相架构（组合/度量/落笔）、算法 |
| 23 | [Atlas 分阶段推进方案](23-Atlas分阶段推进方案-2026-07.md) | **执行**：Stage 0–7、影子对拍、验收口径、退化窗口、待决策项 |

## 一句话

今天的架构把节点坐标和边几何放在两个宇宙里求解，中间用 `FrozenNodeProduct` 一刀切开；边挤不下时无法回头，只能靠 20+ 阶段事后补救。

Atlas 把节点、组框、边通道、标签统一为**占位体**，先做完全部离散决策（**组合相**），再一次求解全部坐标（**度量相**），最后无决策落笔（**落笔相**）。补救阶段不是被优化，是被删除。

## 相关

- 现行结构可视化：[`docs/architecture/layout-routing-architecture.html`](../architecture/layout-routing-architecture.html)
- 经验红线：[`布局与路由核心手册`](../总结经验/布局与路由核心手册-2026-07.md)
- 目录重构（前一轮）：[`doc 34`](../优化重构/34-layout模块结构重构-2026-07.md)
