# Hierarchical · 端口列（PortLane）

> 父页：[architecture](../architecture.md) §7 · 视觉裁定：[expectations §6.2](../expectations.md)  
> 上游：节点 frames（cross + main prelim）· Compose `Ordered`  
> 下游：TrackOrder / Ink（只读展开后的 `AlongSpec`）  
> 状态：**一期已落地**（双胞胎 N/S 走廊；无全局 grid）

## 1. 要解决什么

`Ordered(slot)` 按**各节点 width 比例**展开时，同 slot 在不等宽节点上绝对 x 不同 → Ink 正交展开出现层间横折（`product.three-tier`）。

节点中心共线（SymmetryAxis）**不**消除该折。自由度应上提为 Metric **PortLaneWriter**：为平行走廊写**世界坐标列**，两端共列。

对照 yFiles：`SourcePortAlignmentIds` / `TargetPortAlignmentIds`（路径对齐）。**不是** `PortAssignment.ON_GRID` / 全局 grid 开关（单开 backlog）。

## 2. 写者边界

| 自由度 | 写者 | 不得 |
|--------|------|------|
| side + Ordered 相对序 | Compose | 写绝对像素列 |
| **PortLane 绝对 cross 列** | **PortLaneWriter**（本页） | Ink / Channel 消折特判 |
| `AlongSpec` 像素展开 | Metric（lane 参与端 → `LocalOffset`；其余仍 Ordered×width） | — |
| 折线 | Ink | 发明列位 |

```text
cross + prelim_frames
  → PortLaneWriter
       PortLane[lane_id] → absolute cross
       参与边端 Ordered → LocalOffset（落在该列）
  → TrackOrder / Ink
```

## 3. 一期范围

- 无向端点对上存在正反边（`has_twin`）；  
- 两 real 端点 `span = 1`；  
- 两端 side ∈ {N, S}；  
- 列距 `port_pitch = edge_gap`（世界坐标，非 `t×width`）。

**不做（一期）**：全局 grid；作者 DSL alignment id；节点因 port 加宽；E/W 侧廊。

## 4. 列公式与归属

1. 脊轴 `axis` = 两端节点 frame 中心 x 的均值；  
2. 左列 `axis - port_pitch`，右列 `axis + port_pitch`；  
3. 同一无向对上，按任一端 `Ordered.order`（平局 `EdgeId`）排序：较小 → 左列，较大 → 右列；  
4. 两端改写为 `LocalOffset`，使 `port_anchor` 得到同一绝对 x；x clip 到侧边 margin 内；  
5. 装不下时一期仅 clip（可 diagnostic），不加宽节点。

## 5. 验收

- 不等宽双胞胎：两端 `port_anchor.x` 相等；  
- `product.three-tier`：无因端口 Δx 产生的层间横折；  
- SymmetryAxis D2 门禁不回退。

## 6. 与 grid 的分界

| | PortLane（本期） | 全局 Grid（backlog） |
|--|------------------|----------------------|
| 开关 | 无（双胞胎走廊默认开） | 产品级 gridSpacing |
| 范围 | 平行走廊端口列 | 节点参考点 + 端口贴网 |
| yFiles 对照 | PortAlignmentIds | ON_GRID / ON_SUBGRID |
