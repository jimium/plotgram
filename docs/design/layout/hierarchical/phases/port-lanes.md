# Hierarchical · 端口列（PortLane）

> 父页：[architecture](../architecture.md) §7 · 视觉裁定：[expectations §6.2](../expectations.md)  
> 上游：节点 frames（cross + main prelim）· Compose `Ordered`  
> 下游：TrackOrder / Ink（只读展开后的 `AlongSpec`）  
> 状态：**二期已落地**（一期双胞胎走廊 + §7 共享脸端口对齐；无全局 grid）

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

## 7. 二期 · 共享脸的端口对齐

一期只处理双胞胎走廊。但同样的错位出现在任何**共享脸**上：cross 轴解完之后，两端各自的槽位是按脸宽均分的，与伙伴那一端落在哪一列无关，于是差个十几像素就要在两端各补一折。yFiles 的端口恒在均分槽（导出的 `Ratio` 是 `(2k+1)/2n`），它靠挪节点消掉这段差；我们的层常被压在 `node_gap` 下限上，节点挪不动，只能由脸来吸收。

**写权不变**：Compose 仍独占 side 与相对序，本步只重排**绝对偏移**——它本来就是 PortLane 的自由度。

### 7.1 单元是槽，不是端

同一 `Ordered` 槽上的多个端共用一个 port point（`auto_edge_grouping` 的总线），必须整体移动。投影按槽做，槽内取成员目标的中位数。

### 7.2 只动已经共享的脸

**单槽脸不动**。那一个端口的列就是节点自己的列，属 cross 轴写者；为省一折把它滑到框角，换来的是箭头扎在盒子角上（`smoke.fan-out-four` 上可复现）。只有已经被多个槽瓜分的脸——槽位本就是任意的——才归 PortLane 重排。

### 7.3 目标与投影

每个槽的目标：

```text
长边端 → 自己 dummy 主干的列（已定，Fixed）
短边端 → 与伙伴端的中点（Peer；伙伴若在冻结脸上则直接取伙伴列）
无伙伴 → 保持当前均分槽
```

投影 = 把目标序列压回「Compose 序 + 槽距 ≥ port_pitch + 落在脸内」，用 PAVA。**池化值取中位数而非均值**：同脸两端不能同时到位时，最小二乘各让一半，两条边都留下亚像素抖动、各自还是两折；中位数至少让其中一条精确对齐。

脸之间通过边耦合，所以单趟不是不动点；扫到收敛（位移 < 1e-9）。

### 7.4 二期验收

- `flat/mech.layout-styles`：`sum_bends 66 → 58`、`max_bends 4 → 2`；
- 全 showcase：`Σsum_bends −283`、`Σcrossings −49`，无单点回归；
- `auto_edge_grouping` 总线仍共享一个 port point；
- `smoke.fan-out-four` 的嵌套横杠不塌。

**仍未闭合**：两端都是单槽脸、中心差十几像素的短边（`mech` 里 e3 / e9 一类）。脸上没有可动的自由度，只能靠节点列——而那要求层不被压在 `node_gap` 下限上。归 cross 轴，见 [coordinate-and-demand §8.1](coordinate-and-demand.md)。
