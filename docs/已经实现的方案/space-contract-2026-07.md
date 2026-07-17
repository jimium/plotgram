# 空间契约（Space Contract）— 2026-07

> 基线：[render-layout-routing-baseline-2026-07.md](./render-layout-routing-baseline-2026-07.md) §7.2 / §7.4 / §10  
> 代码入口：`crates/plotgram-core/src/layout/space_budget.rs`

## 一句话

**先预算，再落位，再连线；后处理只守契约，不改拓扑。**

结束「消毒改折线 → 消重叠推节点 → 再重路由」的末端补丁螺旋。

## 契约内容

挂在 `LayoutHints.space_budget: Option<SpaceBudget>`：

| 项 | 含义 |
|----|------|
| `default_node_gap` | 同层默认缝（对齐 architecture `NODE_GAP`） |
| `pair_gaps` | 有 label 的边抬高两端最小缝（如「主从同步」） |
| `port_clearance` | 端口外向 stub，路由/消毒不得反向占用 |
| `corridor_boost_requested` | 路由 0 候选时请求升档外框/走廊 |

不变量：`gap≥预算`、无节点重叠、正交边无反向 stub、有走廊时路径不穿无关节点腹地。

## 管线职责

| 阶段 | 做 | 不做 |
|------|----|------|
| 布局（two_phase / 组内坐标） | `from_diagram` + `enforce_horizontal_gaps` | 事后靠推节点「挤出」标签位 |
| 正交路由 | 消费走廊；退化则 `request_corridor_boost` 再试 / 换端口 | 静默穿障直线、无 stub 脏折线 |
| refine / PRS | 推开后硬间距钳制；移动后 `route_after_node_moves` | 为降交叉压穿 `min_gap` |
| sanitize / snap | 反向 stub + 强制正交 + 真微折 | 大 U 折叠、变 Straight 穿层 |
| 末端消重叠 | **仅** `horizontal_gap_violations` 非空时 `resolve_residual_with_budget` | 无条件 margin=8「刚好不碰」 |

## 与基线债的关系

本方案是基线 **§7.4「空间预算前移 + 反馈环收紧」** 的具体化：

- §7.2 P0「布局↔路由预算被动」→ 由 `SpaceBudget` 显式协商，主修复前移到布局/路由
- §7.4 第 6 步「反馈环收紧」→ refine/PRS 后仅契约失败才兜底推开并增量重路由
- 教训：**几何消毒不是布局策略**；sanitize 行数与职责只减不增

## 验收锚点

- `c.layout-stress-nested`：无节点重叠；`db_master↔db_replica` 缝 ≥ label 预算；无反向 stub
- 同输入确定性不变；phase0 回归 lint 不恶化
