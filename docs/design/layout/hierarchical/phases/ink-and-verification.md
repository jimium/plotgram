# Hierarchical · Ink 与分相验真

> 父页：[architecture](../architecture.md) §11  
> 输入：[contracts-and-ir](contracts-and-ir.md) · [ports-and-channel](ports-and-channel.md) · [coordinate-and-demand](coordinate-and-demand.md)

Ink 的职责是把 `Plan + Metric` 展开成可渲染几何。它可以写最终 path 点列，但不能新增离散决策。

## 1. 输入完备条件

每条 Builtin edge 进入 Ink 前必须已有：

- original source/target；
- source/target PortPlan 与 Metric PortPoint；
- 完整 `RouteTopology`；
- orthogonal 时完整 segment sequence + track order + track coordinate；
- bundle membership；
- arrow/head-tail label 的 original 语义。

任一字段缺失是 `InternalInvariant`。禁止 `unwrap_or(default_side)`、猜中点或临时加 dogleg。

orthogonal 路径**禁止**用两端中点 `mid_y`（或等价缺省）发明水平轨；弯折主轴坐标必须来自 Plan track order + Metric `track_coord`（D₁ 契约见 [channel-d1.md](channel-d1.md)）。缺 track 字段与缺 `RouteTopology` 同等对待 → `InternalInvariant`。

Main 走廊出桩：若在 port 中线水平出轨会穿同层兄弟，可先竖直落到 Metric layer-gap Y 再出轨（逃生）。**入桩优先沿端口法向**——East/West 在 stub 清空时最后一跳水平，North/South 最后一跳竖直；E/W stub 不清时回退 gap 入桩（宁可法向妥协，禁止穿模）。

## 2. 展开流程

```text
1. topology expansion
2. endpoint shape clipping
3. exact orthogonal/polyline point generation
4. bundle trunk joining
5. collinear merge + zero-length removal
6. coordinate quantization / snap
7. corner radius reduction
8. arrow inset / decoration
9. InkVerifier
```

### 2.1 可做

- 按 Plan segment sequence 取 Metric track coordinate；
- 将 PortPoint 接到第一/最后 segment；
- 合并共线点；
- 对已存在的折角做圆角；
- 在短段上降低圆角半径；
- 依据 original arrow 语义缩进 path 端点；
- 进行统一、确定性的数值量化。

### 2.2 不可做

- 改 PortPlan/PortPoint；
- 选择另一条 channel；
- 交换 track order；
- 为避障移动节点/组框；
- 创建未在 BundlePlan 中声明的共享干线；
- 穿越不在 BoundaryCrossing 中的组边界；
- 因曲线样式未实现而静默退化为 orthogonal。

## 3. Polyline

`RouteTopology::Polyline` 必须明确：

```text
Direct
Via(NodeKey|GateKey|GuideKey[])
```

Ink 只把 Direct 展开为两端点，把 Via 展开为已给 guide point。  
“polyline 可不走 Channel”不等于“Ink 自己决定怎么连”。

## 4. Orthogonal

每个相邻点必须共享 x 或 y。端口出针方向由 PortPlan.side 决定：

- North/South 第一段垂直；
- East/West 第一段水平；
- 最小 stub 长度来自 Metric；
- 目标端同理。

**port 邻接段法向契约**：首段必须沿 `from_port.side` 外向法向出发，
末段必须沿 `to_port.side` 外向法向进入——任何切向首/末段（与 node 面
重合）都是缺陷。当 Channel escape 的展开移动本身沿 node 面切向时
（E/W 端口 + `ViaGap` 先移向层隙；N/S 端口 + `AtPortNormal` 先移向
Main rail），Ink 先插入 `port_stub` 法向 stub 再做切向 jog。stub 长度
来自 typed 参数 `port_stub`（默认 12），Ink 侧 clamp：cross-axis stub
≤ `node_gap/2`、main-axis stub ≤ `layer_gap/2`，保证 stub 后的主轴段
只跨越本 rank 带与相邻层隙，不穿同层兄弟。

escape 配对（`EscapePlan`）由 Channel 搜索 J 唯一决定：host 走廊与
escape 方式逐端枚举为候选并计入目标函数，求解器选出配对后直接
写入；Ink 是纯展开者，不做任何事后判定。

若 segment sequence 无法连接端口点，是 Plan/Metric 错误；Ink 不增加修复肘点。

## 5. Bundle

- BundlePlan 中的 shared segment 使用同一几何点列；
- member edge 在 merge/split gate 接入；
- 箭头与 label 仍属于各 original edge；
- 非 BundlePlan 边之间的完全重合一律视为错误。

## 6. 数值规范

- 禁止 NaN/Inf；
- `-0.0` 规范为 `0.0`；
- snap/quantize 使用固定单位与固定舍入规则；
- 共线判断使用项目统一 epsilon；
- 量化只能消除数值噪声，若导致 topology 改变或穿障碍则失败。

## 7. Verifier 分层

### 7.1 PlanVerifier

在 Compose freeze 执行，验证离散事实：

- stable key 唯一；
- layer/order/proper chain；
- original/working edge 映射；
- port/gate/scope；
- route connectivity；
- track precedence 无非法环。

### 7.2 MetricVerifier

在 Metric freeze 执行，验证几何约束：

- finite；
- node/group/partition containment 与 separation；
- Demand 下界；
- port point；
- track order 与容量。

### 7.3 InkVerifier

最低断言：

1. path 至少两个点（合法退化 edge 另有显式类型）；
2. 首尾等于 source/target PortPoint；
3. orthogonal 每段轴对齐；
4. path 不穿非端点 node obstacle；
5. group crossing 与 BoundaryCrossing/GatePlan 一致；
6. 非 bundle 边不完全重合；
7. 圆角半径不大于相邻段允许值；
8. 箭头与 head/tail label 没因 FAS 反向；
9. path 无 NaN、重复零段与未规范化共线点；
10. 首/末段方向等于对应 port 的外向法向（`verify_port_stubs_normal`，
    硬失败；v1 audit E1 的方向部分，不守卫 stub 长度）。

### 7.4 FacadeVerifier

- Layout 与 Router 前后的 nodes/groups/ports bit-identical；
- edge id 集合、顺序与 original graph 一致；
- final canvas 包含所有已定几何；
- 同输入/参数/LayoutData 双跑 bit-identical。

## 8. 错误归因

Verifier 不做 repair，只把错误归到最早写者：

| 现象 | 归因 |
|------|------|
| 端口侧错误 | PortWriter / Orientation |
| 多余绕行或非法 scope | RouteTopoWriter |
| track 交换/重合 | TrackOrderWriter |
| 组框挤边 | Demand / CoordWriter |
| 节点重叠 | CoordWriter |
| Ink 出现新肘点 | InkWriter 违规 |

修复必须回到该 Writer；禁止在更下游添加例外。
