# Hierarchical · 路线

> 目的：现行能力 + 下一步缺口。契约真源：[architecture.md](architecture.md)；相级：[phases/](phases/README.md)；否决路线：[notes/anti-patterns.md](notes/anti-patterns.md)。  
> 写权：[write-authority.md](../write-authority.md)。

---

## 现在

```text
FAS(+环 reroot) → rank / properify / order（含 chain-block sift）
  → ports（FREE 邻层序 / FixedSide）→ Metric（J(x)+VPSC / PortLane / Track）
  → Channel（Substrate + TrackOrder + rip-up + Corridor Allocator）
  → Ink 展开 + diagnostics
```

| 能力 | 写者 | 状态 |
|------|------|------|
| 次轴共线 / 扇对称 | Metric `J(x)` + snap | 已交付 |
| 端口 side / 邻层槽序 / 共享脸 along | Compose + PortLane | 已交付 |
| Channel 走廊 / Gate / 有界 rip-up | RouteTopo + TrackOrder | 已交付 |
| Weak 组框 | Metric；finalize 透传 | 已交付 |
| StrongMacro | MacroBlockWriter；与 Weak 同一 Plan | 已交付 |
| 穿组 verifier | `verify_no_group_penetration` | 已交付（Weak 仍有 C 类豁免） |
| 边参数 + `auto_edge_grouping` | Compose | 已交付 |
| PartitionGrid **引擎消费** | — | 未交付（model/DSL 已备） |

硬不变量由 `hier_eval` 守住。组框单写者按 policy：**Strong = MacroBlockWriter；Weak = Metric**。禁止再叠第二写者。

---

## 下一步

| 缺口 | 观感 | 归属 |
|------|------|------|
| Weak 穿组 **构造**清零（门禁已硬，C 类豁免仍在） | 部分图仍靠 fallback | Channel 基片 / 组通道 |
| `channel-group-fallback` 触发数 | fallback 后等同无组 | 上条 |
| PartitionGrid 消费（泳道 / 矩阵） | 声明了 grid 仍不排带 | [partition-grid](phases/partition-grid.md) |
| `group_anchor` 仍当普通节点分层 | 贴框语义不纯 | 组专项（ADR-004） |
| 次轴：扇出主臂下短链贴父槽 | 检验图相对 yFiles 仍偏左 | P4 `J` / 权重，见 [anti-patterns §4](notes/anti-patterns.md) |
| strong-port projection；label / loop reserve | 端口序不进列位；标签空间未建模 | 余项 |
| GroupMinWidth / title；`close_sibling` 并进 J | D₂ 刻意推迟 | Weak 框余项 |
| architecture profile 默认 strong | 须显式 `group_policy` | 语料够了再评估 |
| 全局 Grid / integrated labeling / 真 MCF | 不挡主路径 | 后置 |

产品优先：**泳道/矩阵 → PartitionGrid**。并行可选：穿组构造、Compose 列位、labeling。

---

## 纪律（不重复 architecture）

1. 单写者；Ink 零新决策；反向只走 DemandBoard。  
2. 多期待拉扯 → 改 `J` / 权重，不加第三趟特判。  
3. 禁止图种分支；参数能 bind 就必须被消费。  
4. 已否决的试法见 [anti-patterns](notes/anti-patterns.md)，不要重开。
