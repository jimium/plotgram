# 笔记：v1 Atlas 求解器 vs `algo::vpsc`

> 日期：2026-07-31  
> 状态：设计态度（现行）  
> 相关：[`plotgram-algo/PARTS.md`](../../../crates/plotgram-algo/PARTS.md) · [from-yfiles-reference](../layout/hierarchical/from-yfiles-reference.md) · v1 `layout/kernel/coordinate/`

---

## 结论（先读这句）

v1 Atlas 求解器 ≈「分隔约束一维坐标核」的**自研投影法**；与 **VPSC 同问题族、不同算法**。  
做 `algo::vpsc` 是升级/统一零件，不是重复发明同名模块。

**问题还在，v1 那套实现不该长期并存。**

| 层 | 以后还要吗 |
|----|------------|
| **「一维分隔约束 + 软目标」度量相** | **要** — Hier 坐标、缝宽、组框、nudging 都靠它 |
| **v1 的 PAVA + Dykstra + 投影梯度整栈** | **不要当第二求解器留着** — 与 `algo::vpsc` 同族重复 |
| **v1 的 IR / 审计 / Demand 接线** | **要迁思路与形状**，不必迁求解内核 |

留下的是**相职责与约束模型**，换掉的是**求解引擎**。

---

## v1 实际是什么

路径：`crates/v1/plotgram-core/src/layout/kernel/coordinate/`

```text
硬约束：x[j] - x[i] ≥ d   （一维分隔）
解法：  PAVA（层内相邻分离投影）
      + Dykstra 交替投影（bounds / 跨层分离）
      + 投影梯度（软目标）
```

Atlas 总纲曾写：逐轴都是「带分离约束的一维凸问题」，并当时选择**不引入**通用 QP/VPSC，沿用 PAVA 栈。

---

## 与经典 VPSC 的对照

| | **VPSC**（`plotgram-algo`） | **v1 PAVA + Dykstra + 投影梯度** |
|--|---------------------------|----------------------------------|
| 问题 | $\min \sum w(x-x^{des})^2$ s.t. 任意一对 $x_j-x_i\ge g$ | 同型分隔约束 + 分层软目标 |
| 硬约束图 | **任意**分离约束（一般约束图） | 层内多为**链上相邻**；跨层/bounds 另投影 |
| 算法核心 | block merge / **split**（乘子） | 保序回归 PAVA + 多凸集交替投影 |
| 最优性 | 该 QP 上论述清晰 | 硬约束靠投影满足；软目标迭代近似 |
| 用途重叠 | 间距、消重叠、nudging、组框边 | Atlas **度量相**主路径（Main/Cross） |

- **问题亲戚**：都是一维变量 + 分隔约束。  
- **算法表亲**：不是换名；VPSC 覆盖更一般的约束图。  
- **不是**：v1 已经实现了完整 VPSC。

---

## 为什么实现层可以换掉

1. 硬约束同型；约束一多（跨层、组框、nudging），v1 靠多投影拼接，VPSC 一份吃一般分离图。  
2. 许多软目标可收成 VPSC 的 **desired + 权重**，不必永挂第二套投影梯度核。  
3. 长期 **PAVA 栈 + VPSC** 双轨 = 双写权、双 bug、双确定性表面，违背零件统一。

---

## 迁移时怎么拆 v1

| 保留 / 上提 | 淘汰（VPSC 吸收后删） |
|-------------|------------------------|
| `CoordinateProblem` 类约束 IR 思路（变量、层分离、跨层、bounds） | `projection.rs` 里 PAVA/Dykstra 作为**主**硬约束求解器 |
| auditor（违反可观测） | 「BK 不够再往 PAVA 上打补丁」的路径 |
| channel demand → gap 的**回写协议** | 与 VPSC 并行的第二坐标宇宙 |
| Main/Cross 分轴、track publish 的**写权** | 事后 group 包围盒当真源（应进约束变量） |

可选过渡：链状分离先作 VPSC 特例（或短期 `project_chain` 快路径）；**对外只暴露一个 `algo` 求解入口**；回归绿后再删 v1 投影核。

---

## 和「度量相还要不要」的区分

- Atlas **度量相**还要：节点框、缝、track、组框变量。  
- 变的是：度量相调用 **`algo::vpsc`（+ 必要的 desired）**，不再维护一套独立 coordinate optimizer 作为永久真源。

**最终态**：一个分隔约束零件（VPSC），多处消费；v1 求解器是历史实现，不是永久双轨。
