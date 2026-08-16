# Circular · 能力范围与典型域

> 父页：[README.md](README.md) · 目标架构：[architecture.md](architecture.md)

## 1. 能力范围（做）

| 能力 | 说明 |
|------|------|
| 单环 | `partitioning: single-cycle`：全体节点一圈 |
| BCC 多环 | 默认 `bcc-compact`：每个双连通分量一圈，割点只属一个分区 |
| 圈序 | 谱序（Fiedler）为首选；声明序 / BFS 作回退；禁止 `HashMap` 序 |
| 变宽节点 | 半径由弧长约束写出：相邻节点外延 + `node_gap` |
| 区内边 | 默认弦（interior）；可选同区外弧 |
| 区际边 | 走块割树骨架；不假装成第二套圈序 |
| 连通分量 | 多个弱连通分量各自成图，再装箱（声明序） |
| 非树/非分层图 | **允许环**；这是本核存在的理由 |

### 1.1 与「图种」的关系

引擎只认 `layout: circular` + typed params（ADR-001）。

| `profile` | 编排层默认 | 说明 |
|-----------|------------|------|
| `er` | 已是 `circular` | 关系团块，不是 ER 专核 |
| `state` | **仍是 `hierarchical`** | 分层状态机走 Hier；环形状态机作者写 `layout: circular` |
| 其它 | 不自动改到本核 | 禁止 `if state` / `if er` 出现在 `plotgram-layout` |

将来若要「state 偏环则填空 circular」，只许在 profile 展开表做，且不得覆盖作者已写的 `layout:`。

## 2. 非目标（故意不做）

| 非目标 | 归属 / 说明 |
|--------|-------------|
| 有向分层 / 减交叉主路径 | → [Hierarchical](../hierarchical/) |
| 组织图 / 导图 / 目录树 | → [Tree](../tree/) |
| 径向树 / balloon 树 | Tree `placer: radial` / `balloon`（已落地）。本核骨架**借用 balloon 几何**排**分区圆**，不把节点树画成径向树 |
| 独立 `RadialLayout` 注册名 | 禁止。一般图同心层不是本核 MVP；树状径向归 Tree |
| CompactDisk / Organic 盘内力导 | yFiles `DISK` / `ORGANIC` / `COMPACT_DISK` 后置 |
| 边捆绑 | 后置；仅 CYCLE + 非 `bcc-isolated` 才和谐（对齐 yFiles 限制） |
| 星形子结构识别 | 后置 |
| 完整 integrated labeling | Demand 预留半径；GENERIC 落位后置 |
| 用 Hierarchical「摆成一圈」冒充本核 | 层流 ≠ 环 |
| 在 Ink 发明圈序、半径、外弧选边 | 写权在 Compose / Metric |

## 3. 典型域

| 域 | 为何适合 Circular | 参数直觉 |
|----|-------------------|----------|
| 网络 / 电信拓扑 | 环与桥（割点）同时可见 | 默认 `bcc-compact` |
| 社交 / 子群 | 团在圈上，桥在骨架 | 同上 |
| 环形状态机 | 闭环转移，无全局 rank | `layout: circular` + `single-cycle` 或 BCC |
| ER 关系图 | 实体团块，弱方向 | `profile: er` |

不适合：流程图（Hier）；组织树 / 导图（Tree）；时序消息（Sequence）。

## 4. 与产品能力的差距（设计记债）

相对 yFiles CircularLayout 的未消费项，契约真源是 **[deferred.md](deferred.md)**（bind 失败类别、写者、落地纪律）。摘要：

1. **分区风格** — 只做 `CYCLE`。`DISK` / `ORGANIC` / `COMPACT_DISK` 后置。
2. **骨架** — 内建 balloon 排分区圆，**不**嵌套 `layout: tree`。共半径 `false`、packed-circle 分量装箱后置。
3. **外弧 / 自动选边** — `interior` 默认；`exterior` 已落地；`automatic` 后置。
4. **from-sketch 圈序** — 后置。
5. **bundling / star / 射线标签 / GENERIC 落位** — 后置。
6. **v1 recipe** — 只读参考（HashMap、图种门面、边交给独立 circular router）。重建禁止这三项。
