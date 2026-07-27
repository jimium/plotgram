# 新架构

布局与路由下一代架构（**Atlas**）的设计与推进文档。

> **当前状态**：设计期结束（Stage 7 收口）。门禁已开闸（[`AGENTS.md`](../../AGENTS.md) §10 已失效；[`gate-switch.sh`](../../benchmarks/scripts/gate-switch.sh) `GATES_DEFAULT=on`）。

## 阅读顺序

| # | 文档 | 作用 |
|---|------|------|
| 21 | [Hierarchical 统一内核与泳道语义](21-Hierarchical统一内核与泳道语义-可行性与演进建议-2026-07.md) | **立场**：要什么、不要什么。group 一等公民、flowchart/arch 去双轨、四布局内核、双路由；DSL swimlane/table 非第一需求 |
| 22 | [Atlas 总纲](22-Atlas下一代布局与路由架构-总纲-2026-07.md) | **设计**：五病灶诊断、B1–B6 公理、术语表、三相架构（组合/度量/落笔）、算法 |
| 23 | [Atlas 分阶段推进方案](23-Atlas分阶段推进方案-2026-07.md) | **执行**：Stage 0–7、影子对拍、验收口径、退化窗口、决策记录 |
| 24 | [Channel 端口挂接增强需求](24-Atlas-channel端口挂接与候选端点增强需求-2026-07.md) | **可执行需求（已落地）**：PortSlot 身份 / 按侧挂接 / 候选端点选路（R1–R5）。实现说明见 [`channel/README.md`](../../crates/plotgram-core/src/layout/atlas/channel/README.md) |
| 25 | [相 I 可行率探针报告](25-Atlas-相I可行率探针报告-2026-07.md) | **实证**：三集 77 图 / 1152 边的粒度、容量与性能测量。旧口径读数修正见 27 号文 §5；**L1–L8 后新口径重采见 §8** |
| 26 | [channel 模块图解](26-Atlas-channel模块图解-2026-07.html) | **图解**：通道图抽象的可视化说明（HTML） |
| 27 | [channel 审查与改造需求](27-Atlas-channel模块审查与改造需求-2026-07.md) | **审查**：粒度与性能结案；合法性缺口 + **L1–L8** 合法化改造（编号刻意不用 R*，以免与 24 号文 R1–R5 撞车） |
| 28 | [channel 合法化实现总结与审查](28-Atlas-channel合法化实现总结与审查-2026-07.md) | **总结（已落地）**：L1–L8 实现定案、A1–A10 验收全过、独立代码审查结论与留债清单 |
| 29 | [路径级作用域自反证 verifier 报告](29-Atlas-路径级作用域自反证verifier报告-2026-07.md) | **证明义务（已落地）**：`verify_route_scope` 独立验证器，A2 证据链升级 + A2b 新指标，三集恒 0。22 号文台账第二项在作用域线上闭环 |
| **30** | [实现检讨：冗余与缺失](30-Atlas实现检讨-冗余与缺失-2026-07.md) | **Stage7 / Post-S7 之后下一刀**：yFiles 第一性原理；M1–M8 / R1–R5；**纠偏**「先 MCF / 整目录清空 ortho」 |
| **31** | [M7 Ink repair 基线](31-Atlas-M7-0-ink-repair基线-2026-07.md) | **测量 + 收口**：product∪stress `distorted=0`；**M7 后期已删** Atlas dogleg repair |

## 一句话

今天的架构把节点坐标和边几何放在两个宇宙里求解，中间用 `FrozenNodeProduct` 一刀切开；边挤不下时无法回头，只能靠 20+ 阶段事后补救。

Atlas 把节点、组框、边通道、标签统一为**占位体**，先做完全部离散决策（**组合相**），再一次求解全部坐标（**度量相**），最后无决策落笔（**落笔相**）。补救阶段不是被优化，是被删除。

## 当前位置

23 号文 **Stage 7 新门禁与收口已完成**；设计期结束。基线 tag：`atlas-final`。

| 已完成 | 内容 |
|--------|------|
| Stage 0–6 | 地基、Plan、度量相、Ink、Dialect、四内核、Plan diff 增量 |
| **Stage 7** | Provenance / phase-api 门禁；`GATES_DEFAULT=on`；删 Legacy；手册/HTML 收口 |

| 记债（不挡出口；下一刀见 [30](30-Atlas实现检讨-冗余与缺失-2026-07.md)） | 内容 |
|------------------|------|
| **M1–M6 / R5** | **已收口**（含真多 slot、orientation、Cross label Demand+诊断对齐、R5 无 bbox seed）；完整 integrated labeling / ortho 整目录仍见 30 |
| **R2** | **已落地**：删 adapter/shadow/compare；`ink_verify` 挂 Hier Ink（**M7-2 已将几何升为硬 FAIL**；见 30 · R2 / 31） |
| **R4** | **已落地**：删 `gate_mcf` / `coord_descent`；删兼容别名；非 Hier 不写假 `atlas_plan`（见 30 · R4） |
| **M7** | **已闭环**：中期守 gate + M7-2 硬门禁（**含 PortSideMismatch 升硬**）+ **后期删** `repair_ink_group_pierces`（见 [31](31-Atlas-M7-0-ink-repair基线-2026-07.md)） |
| **真 MCF / I.7** | 占位模块已删（R4）；未实现，勿当交付 |
| **ortho 物理清空** | Hier 主路径不用 OVG；R1 桥接 + trunk + **外提 stub/sanitize/Endpoint + 死 channel_load**；整目录清空仍待（见 30 · R1） |
| **非 Hier Ink 内化** | Tree/Sequence/Circular 仍 recipe 路由 |
| **M8** | **A+B 已落地**：实验 env 收口 + slots 拓扑同构 skip / 重定位（见 30 · M8）；仍 opt-in，勿默认开 |
| **R3** | **R3-1～R3-5 组合壳已落地**（度量尾段 + L2 + weak/strong expand + 显式 slots）；残余：Horizontal draft 必填 / Super LK（strong）/ `AtlasSolveOutput` 全必填（见 30 · R3） |

## 相关

- 现行结构可视化：[`docs/architecture/layout-routing-architecture.html`](../architecture/layout-routing-architecture.html)
- 经验红线：[`布局与路由核心手册`](../总结经验/布局与路由核心手册-2026-07.md)
- 目录重构（前一轮）：[`doc 34`](../优化重构/34-layout模块结构重构-2026-07.md)
