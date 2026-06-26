# 架构图布局改进方案

## 1. 背景与现状

架构图（architecture）采用两阶段布局算法：组内 Sugiyama 分层 → 组间宏观定位 → 全局坐标回填。Group 作为一等公民参与布局，而非事后包围盒。

经过对 showcase 中 21 个架构图 SVG 的检查，发现 **27 处节点超出 group 边界** 的缺陷，涉及 8 个 SVG 文件。典型表现：

| 文件 | 超出节点 | 超出方向 | 偏移量 |
|------|---------|---------|--------|
| c.cloud-native.svg | Prometheus, Loki | 顶部 | 44px |
| c.k8s-platform-stack.svg | Prometheus, Loki, Mobile App, cert-manager, cart-service 等 | 左/顶 | 20–48px |
| c.ecommerce-platform.svg | CDN, 负载均衡, 支付服务 | 顶/左 | 30–46px |
| c.payment-clearing-platform.svg | 商户接入, Routing Engine, Settlement Engine | 左 | 6–52px |
| n.d2-cell-tower-network.svg | Satellites | 顶 | 46px |

---

## 2. 根因分析

经代码追踪，定位到 **三个独立根因**，共同导致了节点溢出：

### 2.1 V2 友好性调整器与 refine 推开节点后不重算 group bounds

**严重程度：高（主要根因）**

- `FriendlinessAdjuster::apply`（[adjuster.rs:239-242](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/friendliness/adjuster.rs#L239-L242)）为减少边穿障，沿法线方向推开节点，每轮最多 80px。
- `refine::run_refine`（[refine.rs:164-167](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/refine.rs#L164-L167)）为消除边-节点重叠，推开问题节点，每轮最多 40px。
- 两者都只修改 `result.nodes`，**不更新 `result.groups`**。
- 这两步在 grid snap + `refresh_layout_bounds`（重算分组包围框）之后执行，导致 group bounds 反映的是 V2/refine 之前的节点位置。

**管线时序问题**（[mod.rs:837-945](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/mod.rs#L837-L945)）：

```
snap_layout_to_grid          ← 节点移到格点
refresh_layout_bounds        ← 从节点重算 group bounds ✓
snap_group_bounds            ← 量化 group 边到格点
V2 adjuster.apply            ← 推开节点，不更新 groups ✗
router.route                 ← 路由边
refine::run_refine           ← 推开节点，不更新 groups ✗
finalize_canvas_bounds       ← 平移所有元素（保持相对位置）
```

### 2.2 `align_group_borders` 平移 group 边框但不调整 width/height

**严重程度：中**

[grid_snap.rs:213-215](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/grid_snap.rs#L213-L215) 中，`align_group_borders` 对齐同侧 group 边框到中位数时：

- `g.x = median`（左边缘对齐）但 `width` 不变 → 右边缘跟随平移，group 整体偏移
- `g.y = median`（上边缘对齐）但 `height` 不变 → 下边缘跟随平移
- **group 内部节点不被移动** → 节点可能落在 group 边框外

原代码的 `_adjust_far` 参数设计用于补偿，但传入的闭包是 `|g| g.width += 0.0`（no-op），且函数体内从未调用该参数。

### 2.3 `is_simple_chain` 误判 fan-in 为 Vertical

**严重程度：中（影响布局质量，非直接溢出）**

[group_layout_hint.rs:170-208](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/node/architecture_v2/group_layout_hint.rs#L170-L208) 中，`is_simple_chain` 的判定条件为 `in_deg_gt1 <= 1`，允许一个节点有多个入度。

对于 fan-in 模式（如 可观测性组：metrics/logs/traces → grafana）：
- grafana 的 in_degree = 3 → `in_deg_gt1 = 1`
- `out_deg = 3 >= members.len() - 1 = 3` → 满足
- 被误判为 Vertical → 4 个节点排成单列（4 层），组过高

正确布局应为：3 个源节点在同一层，grafana 在下一层（Sugiyama 自动分层）。

---

## 3. 已实施的修复

### 3.1 修复 `align_group_borders` 宽高补偿

**文件**：[grid_snap.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/grid_snap.rs)

对齐左边缘时同步调整 `width`（保持右边缘不变），对齐上边缘时同步调整 `height`（保持下边缘不变）。移除了无效的 `_adjust_far` 参数。

### 3.2 V2/refine 后重算 group bounds

**文件**：[mod.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/mod.rs)

在所有节点位移完成（V2 adjuster + routing + refine）后、`finalize_canvas_bounds` 之前，调用 `refresh_layout_bounds` 从节点位置重算分组包围框。

### 3.3 修复 `is_simple_chain` 拒绝 fan-in

**文件**：[group_layout_hint.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/node/architecture_v2/group_layout_hint.rs)

将判定条件从 `in_deg_gt1 <= 1` 改为 `in_deg_gt1 == 0`。任何节点有多个内部入度时，不再视为链式，回退到 Sugiyama 分层。

### 3.4 验证结果

- 全部 638 个单元测试通过
- 21 个架构图 SVG 重新渲染后，**节点超出 group 边界的违规数从 27 降至 0**

---

## 4. 下一步改进方向

以下改进项已全部实施完成。

### P0：布局质量验证体系 ✅

**问题**：当前缺乏自动化布局质量回归检测，节点溢出问题存在已久但未被发现。

**方案**：
1. 在 CI 中增加布局质量快照测试：对 showcase 中所有 .dfy 渲染后检查节点是否在 group 内
2. 在 `LayoutResult` 上增加 `validate_group_containment()` 方法，返回违规列表
3. 将检查集成到 `drawify validate` 子命令

**实施**：
- `LayoutResult::validate_group_containment()` 检查所有 group 的直接实体和子组是否在边界内（1px 容差）
- `drawify validate --layout-check` CLI 子命令
- `showcase_architecture_group_containment` 集成测试覆盖全部 21 个架构图

### P1：Uniform sizing 策略下的 group bounds 一致性 ✅

**问题**：`refresh_groups_with_sizing` 对 `Uniform` 策略跳过重算（`preserve_uniform` 提前返回），V2/refine 推开节点后 Uniform 组的 bounds 仍然过期。

**方案**：
- Uniform 策略下，先从节点重算 bounds，再重新拉齐到最宽组宽度

**实施**：移除 `refresh_groups_with_sizing` 中的 `preserve_uniform` 提前返回，始终从节点位置重算后再应用 Uniform 拉齐。

### P2：Group 内布局模式增强 ✅

**问题**：当前 `GroupLayoutMode` 支持 Horizontal / Vertical / FanOut / Sugiyama 四种，但缺少：
- **FanIn**：多源 → 单汇（当前回退到 Sugiyama，但可做更紧凑的专用布局）
- **Grid**：规则网格排列（适用于无内部边的同质节点组）

**方案**：
1. 新增 `GroupLayoutMode::FanIn { sink }`，sink 在下，源节点在上层水平展开
2. 新增 `GroupLayoutMode::Grid`，按行列网格排列
3. 在 `detect_auto_mode` 中增加 fan-in 检测：单一汇聚点 + 多源

**实施**：
- DSL 新增 `layout: fan-in` 和 `layout: grid` 属性值
- `GroupLayoutHint::FanIn` / `GroupLayoutHint::Grid` 枚举变体
- `GroupLayoutMode::FanIn { sink }`：源节点在 rank 0，sink 在 rank 1
- `GroupLayoutMode::Grid`：按 ID 排序后填入 `ceil(sqrt(n))` 列网格
- `detect_auto_mode` 自动检测：`max_fanin >= 2 && max_fanout < 2` → FanIn；4+ 孤立节点 → Grid
- `pick_fan_in_sink` 优先选 pure sink（出度为 0、入度最大）

### P3：嵌套 group 的空间利用率 ✅

**问题**：容器 group（纯子组无直接实体）的 padding 叠加导致内层空间紧张。`GroupPadding::uniform` 对每层都加 padding + header，3 层嵌套时有效内容区不足 60%。

**方案**：
- 容器 group（无直接实体）使用更小的 padding（如 12px vs 28px）

**实施**：
- `compute_group_bounds` 内部区分 leaf 组和 container 组
- container 组 padding：x 减半（14px），y_top 降至 60%（29px），底部 padding 减半
- 通过 `container_padding()` 从 leaf padding 自动推导，无需调用方修改

### P4：跨组边的组内端口优化 ✅

**问题**：`nudge_intra_nodes_toward_cross_group_edges` 最大位移 24px，对宽组效果有限。跨组边仍可能从组的中部出发，产生不必要的折弯。

**方案**：
- 根据跨组边方向动态调整组内节点的 x 位置，而非固定 24px
- 对多条同向跨组边，按目标 x 坐标排序组内节点

**实施**：
- 动态位移 = `min(可用宽度 × 0.3, |距离| × 0.3, 48px)`，下限 16px
- 跳过组内 hub（有组内后继的节点），避免破坏 hub 居中
- 同组节点按目标 x 排序后强制最小间距（NODE_GAP），防止重叠
- 双向扫描（正向 + 反向）确保无溢出

### P5：Group 间距自适应 ✅

**问题**：当前 `GROUP_GAP_X = 50px` 为固定值。组间边密集时间距不足导致边重叠；组间无边时间距过大浪费空间。

**方案**：
- 按组间边数量动态调整间距：`gap = base + edge_count * factor`
- 无边的相邻组自动靠拢

**实施**：
- `adaptive_group_gap()` 统一间距计算
- 有边对：`GROUP_GAP_X + min(count × 6, 40)`（原逻辑保留）
- 无边对：`GROUP_GAP_X × 0.5`（自动靠拢至 25px）
- 同时应用于顶层 `position_macro_blocks` 和容器组内定位

---

## 5. 架构图布局能力总结

### Group 当前能力

| 能力 | 实现状态 | 说明 |
|------|---------|------|
| 嵌套分组 | ✅ | `GroupTree` 递归布局，支持任意深度 |
| 组内布局模式 | ✅ | Auto/Horizontal/Vertical/FanOut/FanIn/Grid，Auto 自动推断 |
| 组宽策略 | ✅ | Fit（贴合内容）/ Uniform（等宽拉齐） |
| 跨组边布局 | ✅ | 超级节点图 + 宏观分层 |
| 组内边路由 | ✅ | 组内 Sugiyama 分层 |
| 组框像素对齐 | ✅ | grid snap 量化到 8px 格点 |
| 组边框对齐 | ✅（已修复） | 同侧边框统一到中位数 |
| 容器组紧凑 padding | ✅ | 无直接实体的容器组 padding 减半 |
| 跨组边端口动态微调 | ✅ | 按目标距离和组宽动态计算位移（16–48px） |
| 组间距自适应 | ✅ | 有边对加宽、无边对靠拢 |
| 布局质量验证 | ✅ | `validate_group_containment` + CLI `--layout-check` |

### Group 语义

- Group 是布局的一等参与者，在 Phase A（组内布局）前就确定
- Group 的尺寸由组内内容驱动（Fit）或全局拉齐（Uniform）
- Group 的位置由宏观分层（Phase B）决定，组内节点在组框内相对定位
- Group bounds 在 grid snap 后从节点位置重算，确保始终包含所有成员
