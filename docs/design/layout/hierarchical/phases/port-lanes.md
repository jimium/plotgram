# Hierarchical · 端口列（PortLane）

> 父页：[architecture](../architecture.md) §7 · 视觉裁定：[expectations §6.2](../expectations.md)  
> 上游：节点 frames（cross + main prelim）· Compose `Ordered`  
> 下游：TrackOrder / Ink（只读展开后的 `AlongSpec`）  
> 状态：**一期已落地**（双胞胎 N/S 走廊对齐 + 触及脸级绝对 along；无全局 grid）

## 1. 要解决什么

`Ordered(slot)` 按**各节点 width 比例**展开时，同 slot 在不等宽节点上绝对 x 不同 → Ink 正交展开出现层间横折（`product.three-tier`）。

节点中心共线（SymmetryAxis）**不**消除该折。自由度应上提为 Metric **PortLaneWriter**：

1. 走廊两端 **共世界列**（对齐）；  
2. 一旦触及某 `(node, side)`，该脸**全部**端点写绝对 along，**服从** Compose 相对序——禁止「走廊 LocalOffset、同脸其它端仍 Ordered×width」夹心交叉。

**对齐 ≠ 中轴双极**：不得用 `axis ± port_pitch` 强制绕节点中心摆 twin 而打乱同脸总序。

对照 yFiles：`SourcePortAlignmentIds` / `TargetPortAlignmentIds`（路径对齐）。**不是** `PortAssignment.ON_GRID` / 全局 grid 开关（单开 backlog）。

## 2. 写者边界

| 自由度 | 写者 | 不得 |
|--------|------|------|
| side + Ordered 相对序 | Compose | 写绝对像素列 |
| **走廊对齐列 + 触及脸绝对 along** | **PortLaneWriter**（本页） | Ink / Channel 消折 / 消交叉特判 |
| 未触及脸的 `AlongSpec` | 仍为 Compose Ordered（Metric 按框宽展开） | PortLane 改写未触及脸 |
| 折线 | Ink | 发明列位 |

```text
Compose Ordered（相对序）
  → frames
  → PortLaneWriter
       1) 合格走廊：两端共 lane_x（对齐）
       2) 触及脸 (node,side)：该脸全部端点 → LocalOffset
       3) 绝对 x 单调服从 Ordered；走廊块连续；块内分隔 ≥ port_pitch
       4) 非走廊端只落在走廊块外侧的残余区间（不插入两 twin 列之间）
  → TrackOrder / Ink
```

## 3. 一期范围

- 无向端点对上存在正反边（2-cycle / twin）；  
- 两 real 端点 `span = 1`；  
- 两端 side ∈ {N, S}；  
- 块内列距下限 `port_pitch = edge_gap`（世界坐标，非 `t×width`）；  
- **触及脸闭合**（上节）。

**不做（一期）**：全局 grid；作者 DSL alignment id；节点因 port 加宽；E/W 侧廊。

## 4. 算法要点

1. 收集合格走廊组；组内边按任一端 `Ordered.order`（平局 `EdgeId`）排序。  
2. **触及脸** = 走廊任一端所在 `(elem, side)`；列出该脸全部边端，按 Compose 序排序。  
3. **临时比例位**：`x_i = left + (i+1)/(n+1)·width`（与 Ordered 展开同构）。  
4. **对齐**：每条走廊边 `lane = mean(临时位_src, 临时位_tgt)`；组内两列若间距 `< port_pitch`，绕中点撑到 `port_pitch`（保持序：较小 order → 较小 x）；clip 到两端 frame 交集。  
5. **脸级写回**：走廊端钉 `lane`；非走廊端按序落在块左侧或右侧残余区间（`Ordered` 落在两走廊序之间的端视为块右侧，避免夹心）；全部改为 `LocalOffset`。  
6. 未触及脸不改。

纯双胞胎脸（仅两槽）退化为「比例对齐 + pitch 下限」，不再使用中轴双极公式。

## 5. 验收

- 不等宽双胞胎：两端 `port_anchor.x` 相等；  
- `product.three-tier`：无因端口 Δx 产生的层间横折；  
- 同脸 twin + 外叶：外叶绝对 x 不在两走廊列之间（`mech.constrain-sink` 门禁）；  
- SymmetryAxis D2 门禁不回退。

## 6. 与 grid 的分界

| | PortLane（本期） | 全局 Grid（backlog） |
|--|------------------|----------------------|
| 开关 | 无（双胞胎走廊默认开） | 产品级 gridSpacing |
| 范围 | 对齐列 + 触及脸绝对 along | 节点参考点 + 端口贴网 |
| yFiles 对照 | PortAlignmentIds | ON_GRID / ON_SUBGRID |
