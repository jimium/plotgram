# Hierarchical · Weak 组框写权

> 父页：[architecture](../architecture.md) §8.3  
> 坐标相：[coordinate-and-demand](coordinate-and-demand.md) §6  
> 对照：[strong-macro](strong-macro.md)（Strong 框 = MacroBlockWriter，**不**进本文）  
> 否决：[notes/anti-patterns.md](../notes/anti-patterns.md)  
> 状态：**已落地**。余项见文末。

## 1. 一句话

仅 **Weak**：组框由 **Metric 写出真源**；`finalize` **不得**再 `union(members)+pad` 发明框。兄弟分隔与穿组由构造 + 共用 verifier 可证。

不是 StrongMacro 舞台，也不是再开一套 Channel。

## 2. 写权

```text
Compose（boundary / order / ports）
  → Channel（读节点帧 + 组壳）
  → Metric：节点 J(x)+VPSC + 组框变量（含约束 member ⊆ frame；sibling gap）
       写出 node frames + GroupPlacement[]
  → Ink 只读框
  → finalize：`owns_group_frames` → 透传；禁止重算真源
```

Strong 路径不走本 Metric 框变量；MacroBlockWriter 经 `TailFrames::Fixed` 注入尾部。

| 自由度 | 写者 | 禁止 |
|--------|------|------|
| 节点坐标 | Metric `J(x)`+VPSC | Ink / finalize 挪节点消框债 |
| **组框（Weak）** | **Metric 组框 Writer** | finalize `union+pad`；Ink 平移组 |
| 组框（Strong） | MacroBlockWriter | 本文 VPSC 框叠到 Strong |
| 兄弟框缝 | Metric 硬分隔 | 第三趟 compact 作主路径 |
| Gate 拓扑 | Channel | D₂ 另起路由栈 |
| Gate 容量 → 缝宽 | DemandBoard | 静默 fallback |
| 穿组检查 | `verify_no_group_penetration` | post-Ink 刚体推开 |

`LayoutOutput.groups` 由 layout 填写；`owns_group_frames: true` → finalize 禁止兜底重算。

## 3. 明确禁止

1. Strong 路径叠 VPSC 组框变量。  
2. Ink / Channel 模块内读 `group_policy`。  
3. post-Ink / post-finalize 平移组框「修」sibling / containment。  
4. 静默 `channel-group-fallback` 且不进 `relaxations`。  

## 4. 模块

```text
hierarchical/group_frame.rs     // pad / GROUP_FRAME_GAP / label band
metric/symmetry_objective.rs    // Weak-only compact / snap_boundaries
demand.rs                       // LayerGap / 组相关下界
ink/verify.rs                   // verify_no_group_penetration
plotgram-engine finalize        // 透传 groups
```

## 5. 余项（不挡已关闭的写权）

- Weak 穿组 **构造**清零：跳组 Main 定向 + N/S ViaGap + 同线直穿过滤 + 落地 Main 不得跨外国组（绕行走顶/底缝，缝的 Cross 停在组框外）。已摘 hybrid / blue-green。剩余 C-lane：tenant / platform / plotgram / multi-ns / supply-chain 同层横线。  
- GroupMinWidth / title 美学。  
- `close_sibling_frame_slack` 并进 `J(x)`（现为投影后 compact）。  
- Gate 容量公式与 Demand 下界已可观测；触发数下降仍开放。
