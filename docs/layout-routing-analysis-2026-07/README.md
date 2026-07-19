# Plotgram 布局与路由质量分析报告（2026-07）

> 数据来源：对 `showcase/` 全量 **84 个 `.pgm`** 用当日 release 二进制重新评估
> （`plotgram-eval baseline showcase/`），并对最差案例渲染 PNG 逐张人工核查。
> 配套的局部问题标注见同目录 [`local-issues.html`](./local-issues.html)。

---

## 1. 结论速览

| 维度 | 现状 | 判定 |
|------|------|------|
| 节点重叠 | 全量 **0 对** | ✅ 已解决 |
| 边穿节点 (`edge_node_crossings`) | 全量合计 **77**，集中在 architecture（15 个文件） | ⚠️ 主要缺陷 |
| 边交叉 (`edge_crossings`) | 全量合计 **389**，38/84 文件有交叉 | ⚠️ 主要缺陷 |
| 非语义平行边重叠 | 合计 **53**，单图最高 33（mod-deps） | ⚠️ 路由缺陷 |
| 边穿分组内部 | 2（`er/c.saas-schema`） | 🟡 少量 |
| 标签遮挡 | node 19 + label-label 4 | 🟡 少量 |
| 画布宽高比 / 空间利用 | flowchart/state 有 ar>4、util<2% 的极端拉伸 | ⚠️ 紧凑性缺陷 |

**一句话**：正确性的“硬伤”（重叠）已消除，当前瓶颈是 **架构图的跨组长边路由**（绕行、穿节点、穿组、平行重叠）与 **ER/flowchart/state 的二维摆放不足**（单列堆叠导致长边与画布空转）。

### 各图类型评分（自定义 per-type 权重）

| 类型 | 样本 | 平均分 | 最低分 | 最差案例 |
|------|-----:|-------:|-------:|----------|
| mindmap | 8 | **82.2** | 76.7 | `c.knowledge-map` |
| er | 8 | 76.9 | 60.9 | `c.saas-schema` |
| sequence | 14 | 69.7 | 59.6 | `c.ai-agent-change-loop` |
| architecture | 24 | **63.7** | 53.6 | `c.k8s-platform-stack` |
| state | 12 | 63.7 | **43.9** | `c.layout-stress-transitions` |
| flowchart | 18 | 62.7 | 50.3 | `c.e-commerce-order-fulfillment` |

> mindmap（organic）与 er（spline）观感尚可；**architecture / flowchart / state 是三块洼地**。

---

## 2. 按图类型的问题诊断

各类型的默认算法（`crates/plotgram-core/src/profile/mod.rs`）：

| 类型 | 布局算法 | 边路由 |
|------|----------|--------|
| flowchart | `flowchart` | `orthogonal` |
| architecture | `architecture`(v2) | `orthogonal` |
| state | `state` | `circular` |
| er | `er` | `spline` |
| mindmap | `mindmap` | `organic` |
| sequence | `sequence` | 内置（布局产出几何） |

### 2.1 Architecture —— 跨组长边是头号问题（占全部边穿节点/交叉的绝大多数）

最差样本指标：

| 文件 | 边穿节点 | 边交叉 | 平行重叠 | 通道拥挤 | 端口冲突 |
|------|--------:|------:|--------:|--------:|--------:|
| `c.k8s-multi-namespace-overview` | 6 | **72** | 2 | 37 | 603 |
| `c.k8s-multi-cluster-federation` | 9 | 52 | 6 | 33 | 210 |
| `c.k8s-platform-stack` | **10** | 30 | 3 | 24 | 380 |
| `c.plotgram-core-mod-deps` | **10** | 29 | **33** | 29 | 248 |
| `c.k8s-tenant-isolation` | 6 | 30 | 0 | 31 | 380 |
| `c.cloud-native` | 7 | 3 | 0 | 26 | — |

**观察到的具体现象**（详见 HTML 标注）：

1. **长边绕画布外圈走线**：分组沿单一竖轴堆叠，跨组边（如 `platform-system → 各命名空间` 的 deploy 边、`stateful-data` 的数据依赖）被路由成贴着画布左右边缘、上下横跨全图的巨型 L 形/U 形折线。多条这样的长边在左侧、右侧形成“近平行走线带”，互相叠压。
2. **边穿过无关节点**：在密集命名空间内部，正交路由未把“非端点节点”当作障碍，直线段直接压过 `user-api`、`pay-core`、`theme/builtin…` 等节点框与其文字（`mod-deps` 中多处文字被横线划穿）。
3. **边穿过 / 贴着分组边框**：跨组边频繁切入无关分组内部或压在分组边框上（`cloud-native` 的嵌套组 `Kubernetes 集群 / 应用 Pod / 平台组件` 尤为明显）。
4. **非语义平行边未做 trunk 分离**：`mod-deps` 的 33 次平行重叠，是大量指向同一层的长边在同一间隙里叠放，肉眼无法区分是几条边。
5. **分组内部空旷**：`k8s-multi-namespace-overview` 面积利用率仅 **2.8%**，分组框巨大但节点稀疏，进一步拉长了跨组走线。

**根因（映射到代码）**：

- 摆放：`layout/node/architecture_v2/`（`group_layout_hint.rs` / `two_phase`）倾向把顶层分组**竖向单列堆叠**，跨组边天然很长；组内 `intra_sugiyama` 排布留白偏大。
- 路由：`layout/edge/edge_routing_orthogonal/`
  - `corridor_route.rs` / `lane_assignment.rs` 在源/目标组相距很远时，走线退化为沿外圈的长通道；
  - 障碍集合未把“分组框 + 非端点节点”统一纳入 → `sanitize.rs` / `conflict_reroute.rs` 无法消除穿节点；
  - `semantic_trunk_merge.rs` 只合并语义相关边，**`edge_bundling/` 目录当前为空**，没有对无关平行长边做真正的 bundling / 通道分槽。
- 分组避让：`layout/group/corridor.rs`、`post_route.rs`、`layout/post_route/border_repulse.rs` 对“边贴框/穿组”的兜底不足。

### 2.2 ER —— 单列摆放 + 曲线 ribbon

| 文件 | 边数 | 拐点(采样) | 边交叉 | 穿组 |
|------|----:|----------:|------:|----:|
| `c.layout-stress-dense` | 16 | **245** | 8 | 0 |
| `c.saas-schema` | 10 | 152 | 1 | **2** |
| `n.social-network` | 8 | 94 | 3 | 0 |

- **实体几乎排成一列/一竖轴**（`layout/node/er/mod.rs` 是简单摆放），所有“非相邻表”的关系边只能从侧面大弧线绕行，多条 spline 在左右两侧**汇成重叠的曲线带**，无法追踪某条关系连了谁。
- `saas-schema` 有 2 条边穿过分组内部（`核心身份` / `工作区`）。
- 关系菱形标签（`关注`/`上架`/`领取`…）与基数标签（`1`/`N`）散落在曲线中段，部分重叠。
- ⚠️ **指标说明**：`bend_count` 对 spline 是按采样折线统计方向变化，曲线天然“拐点”多，245 更多是**度量假象**；真实问题是 ribbon 重叠与摆放，而非折点。

### 2.3 State —— 秩间空洞造成画布空转（全量最低分 43.9）

`c.layout-stress-transitions`：`aspect_ratio=4.49`、`area_utilization=1.5%`。

- `初始化`、`结束` 被放在最顶端，主状态簇却被压到画布最底部，中间**一条边纵贯上千像素的空白**，形成巨大死区。
- 根因：`layout/node/state/` 的 rank 分配遇到自环（`自环`）与回边时产生了**巨大的空 rank 间隙**，且缺少空 rank 压缩 / initial-final 就近安置。

### 2.4 Flowchart —— 纵向过度拉伸 + 决策段交叉

| 文件 | 宽高比 | 边长 CV | 边交叉 |
|------|------:|-------:|------:|
| `c.e-commerce-order-fulfillment` | 4.29 | — | 0 |
| `c.caffe-shop` | 4.28 | 1.70 | 1 |
| `c.customer-refund-process` | 3.62 | 1.27 | 8 |
| `c.ci-cd-security-pipeline` | 3.72 | 1.56 | 5 |

- 多阶段/多决策流程被排成**极窄的单列**（每个 phase group 内部也几乎单列），宽高比 >4，横向空间浪费、纵向滚动很长。
- 决策密集段（如 refund 的 `审核阶段`）里，`否` 分支向右折返再汇入，产生多处交叉；长 `否` 边贴左侧从中段一路折到底部阶段。
- 布局本身正确（0 重叠、0 穿节点），属于**紧凑性 / 可读性**问题，而非正确性。

### 2.5 Sequence —— 天然瘦高，正确性好

- 序列图 util 普遍 3–5%、ar 2–2.5，是**图类型固有**（生命线纵向展开）；正确性子分高，不是缺陷。可选优化是减少空白高度。

### 2.6 Mindmap —— 当前最佳

- organic 路由 + 径向摆放，平均 82 分，无系统性问题。可作为“均匀性/美观性”标杆。

---

## 3. 全局问题清单（按严重度）

| # | 问题 | 影响面 | 严重度 |
|---|------|--------|:------:|
| 1 | 架构图跨组长边绕外圈、穿节点、穿组框 | architecture 15+ 文件 | 🔴 高 |
| 2 | 无关平行长边未 bundling（`edge_bundling/` 空实现） | architecture | 🔴 高 |
| 3 | state 秩间空洞导致画布空转 | state 压力图 | 🔴 高 |
| 4 | ER/架构 顶层实体/分组单列堆叠 → 长边 + 低利用率 | er, architecture | 🟠 中 |
| 5 | flowchart 多阶段/决策图纵向过度拉伸 | flowchart 复杂图 | 🟠 中 |
| 6 | 标签遮挡节点（尤其架构长边中段标签） | 14 文件 | 🟡 低 |
| 7 | `bend_count` 对 spline/organic 曲线过度计数（度量假象） | er, mindmap | 🟡 低（度量） |

---

## 4. 优化路线图（按 ROI 排序）

### P0 —— 正确性（最高优先）

1. **正交路由统一障碍模型**：把“分组框 + 所有非端点节点”纳入同一障碍集，供 `conflict_reroute.rs` / `sanitize.rs` 消除穿节点、穿组、贴框。
   - 目标：architecture 的 `edge_node_crossings` 77 → 接近 0；`edge_through_groups` → 0。
2. **实现真正的平行边 bundling / 分槽**：填补空的 `edge_bundling/`，对同向长边按目标分组做 trunk 归并与等距分槽（区别于仅语义合并的 `semantic_trunk_merge`）。
   - 目标：`edge_parallel_overlap_count` 53 → <10。
3. **state 秩压缩**：空 rank 折叠 + initial/final 就近安置，消除纵向死区。
   - 目标：`c.layout-stress-transitions` util 1.5% → >8%，ar 4.49 → <2.5。

### P1 —— 可读性 / 紧凑性

4. **架构分组二维装箱**：用 shelf / 分层网格替代顶层分组的竖向单列，缩短跨组边、提高利用率（util 2.8% 是明显信号）。
5. **ER 二维摆放**：按 FK 邻接做力导向/网格摆放，让强关联表相邻；关系与基数标签贴近端点放置。
6. **flowchart 宽高比自适应**：对深链/多决策图允许分支横向展开或多列折叠，把 ar>4 压到黄金比附近。

### P2 —— 度量与打磨

7. **修正 `bend_count`**：对曲线路由改为统计“语义折点/控制点”，而非采样方向变化，避免 er(245)/mindmap 的假象干扰打分。
8. **长边中段标签避让**：标签沿边寻找无遮挡锚点（几何冻结后作为末步，符合手册“避让放管线末尾”原则）。

> 遵循 `docs/总结经验/布局与路由核心手册-2026-07.md`：几何清理/避让放在管线末尾；对比节点坐标不变 + 全量 showcase 重叠严重度量化“无退化”，禁止图名特判。

---

## 5. 复现方式

```bash
# 重新生成评估基线（84 文件全量指标）
cargo build --release -p plotgram-eval
./target/release/eval baseline showcase/ -o /tmp/showcase-fresh.json

# 渲染单个最差案例为 PNG 复核
PLOTGRAM_FONTS_DIR=fonts ./target/release/plotgram \
  render showcase/architecture/c.k8s-multi-namespace-overview.pgm -f png -o /tmp/a.png

# 单图静态质检
./target/release/plotgram lint showcase/architecture/c.k8s-platform-stack.pgm
```
