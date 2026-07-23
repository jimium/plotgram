# A 族修复总结：Port-Stub-Approach 三段几何契约

> 日期：2026-07-22 ~ 2026-07-23
> 覆盖问题：ISS-001（出组前转弯/不对称）、ISS-002（箭头倒悬）、ISS-008（回环折点过多）、ISS-009b/c（跨泳道 U 形/绕远）
> 方案文档：[`边路由与标签问题通盘修复方案-2026-07.md`](../方案计划/边路由与标签问题通盘修复方案-2026-07.md)

---

## 1. 核心结论

A 族 5 个问题**不是 5 个独立 bug**，而是「端口决策被多级互相覆盖的启发式瓜分、缺乏统一几何契约」的外在表现。修复策略为：

1. **建立只读诊断基线**（A-0）→ 量化现状
2. **逐步引入契约约束**（A-1~A-5）→ 每步独立验证、独立采基线
3. **通用几何规则**，零图名特判

最终效果：全 product 集 `contract_unnatural_to_port` 从 57 降至 39（-32%），目标图全部归零；穿组/确定性零回归。

---

## 2. 各步骤摘要

### A-0：只读契约诊断（零几何改动）

- 实现 [`contract.rs`](../../crates/plotgram-core/src/layout/edge/edge_routing_orthogonal/contract.rs) 四个信号：`stub`（出组前转弯）、`approach`（末段方向不符，恒 0）、`unnatural_to_port`（目标端口非自然侧）、`away_segs`（远离段）
- 关键发现：`approach` 恒为 0 → sanitize 已强制契约②，ISS-002 根因不在末段几何而在**端口选错方向**

### A-1：回环近对齐快捷路径（修 ISS-008）

- **根因**：`straighten_preferred_alignments` 不检查目标切线是否被障碍阻挡，把锚点从干净走廊挪到被挡位置 → Z 形绕行
- **修复**：[`straighten.rs`](../../crates/plotgram-core/src/layout/edge/edge_routing_orthogonal/straighten.rs) 新增 `straight_path_blocked` + `should_skip_blocked_alignment`（仅当目标切线被挡**且**被移动端原切线干净时才跳过对齐）
- **效果**：refund `revise→submit` Z 形 → Top→Right 单折 L 形（away_segs 2→1）
- **教训**：首版一刀切跳过致 password-reset 共线回归（exact 11→38），以「原切线同样被挡则不跳过」修正

### A-2：远离惩罚评分（防护栏）

- 实现 [`scoring.rs`](../../crates/plotgram-core/src/layout/edge/edge_routing_orthogonal/scoring.rs) 曼哈顿距离增量加权惩罚
- 验证为 **no-op**（U 形随 task-bb3 布局变化消失），保留为契约③防护栏

### A-3：Stub/Approach 契约 + sanitize 校验（防护栏）

- [`path.rs`](../../crates/plotgram-core/src/layout/edge/edge_routing_orthogonal/path.rs)：`source_group_exit_stub_len` 跨组边出组 stub 候选
- [`scoring.rs`](../../crates/plotgram-core/src/layout/edge/edge_routing_orthogonal/scoring.rs)：`stub_inside_group_penalty` 组内转弯惩罚
- [`corridor_route.rs`](../../crates/plotgram-core/src/layout/edge/edge_routing_orthogonal/corridor_route.rs)：`group_exit_stub_len` 首段出组
- 验证为 **no-op**（当前 trunk-merge 产出路径已满足契约①），保留为防护栏
- 真正残留缺陷归 A-5（端口决策）

### A-4：对称锚点配对（已满足）

- 验证 microservices web/mobile→gateway 锤点关于 trunk x=202 镜像对称（±32），gateway 扇出对称（±8）
- 对称性随 task-bb3 + DET 确定性修复后自然满足，**无需代码改动**

### A-5：近对角守卫（修 ISS-002，核心修复）

- **根因**：`stub_fix.rs` 的 `detect_side_approach` 只看局部 jog（152px 水平 > 54px 竖直 stub），对近对角边（order_svc→db, dx=-168≈dy=+166）错误触发 Rotate(Left/Right)，且侧向路径更短被接受 → 端口翻成 Left→Right → db 处箭头倒悬
- **修复**：[`stub_fix.rs`](../../crates/plotgram-core/src/layout/edge/edge_routing_orthogonal/stub_fix.rs) 新增 `side_approach_axis_dominant` 近对角守卫——仅当建议侧轴向在边整体几何上超 2× 占主导（与 `contract.rs::natural_to_port` 同口径）时才允许旋转
- **效果**：
  - microservices `order_svc→db` Bottom→Top（unnat 1→0），全 8 边 ok=true
  - 全 product 集零回归，**附带改善 9 图**（typical-microservice 9→4、ecommerce 11→10、symmetric-fanout 4→0 等）
  - 穿组不变（cdn-cache=1、saas-schema=2 为基线遗留），全样本 det=true

### DET：确定性修复（flat-mesh 非确定性）

- **根因**：`channel_planner.rs` 的 `collect_channel_candidates` 从 HashMap keys 无序收集 node/group ids → `find_best_channel` 平局按 Vec 序取胜 → 路径分叉
- **修复**：排序 `group_ids`/`node_ids` 后再收集候选
- **验证**：连跑 30+ 次 byte-一致

---

## 3. 量化结果

| 指标 | 修复前（A-0 基线） | 修复后 | 变化 |
|------|-------------------|--------|------|
| product 集 unnatural_to_port 总计 | 57 | 39 | **-32%** |
| microservices unnat | 1 | 0 | ✓ |
| swimlane unnat | 1 | 0 | ✓ |
| refund unnat | 4 | 2 | -50% |
| 穿组（edge_crosses_group_interior） | 3 | 3 | 不变（基线遗留） |
| 确定性（det） | flat-mesh 15-20% 不确定 | 全 true | ✓ |

---

## 4. 关键经验

1. **先追管线时序，再调局部启发**：ISS-002 表面是"端口选错"，实际是 4b stub_fix 把正确的端口翻错了。不追写权时序就会在 port_solver 上白费功夫。
2. **no-op 防护栏有价值**：A-2/A-3 验证为 no-op，但作为契约防护栏防止未来回归——布局变化（如 task-bb3）可能重新触发这些路径。
3. **通用几何规则 > 图名特判**：A-5 的 2× 轴向占主导判据与 `natural_to_port` 同口径，一处修改惠及全 product 集 9 图。
4. **确定性是隐蔽杀手**：DET 修复前，任何"连跑两次对比"的验证都不可信——flat-mesh 15-20% 概率不同。
5. **隔离基线排除干扰**：A-1 用 before-a1 vs a1-refined 隔离基线，排除 task-bb3 布局变化的干扰，精确归因。

---

## 5. 遗留与后续

| 项 | 状态 | 归属 |
|----|------|------|
| ISS-003 形状轮廓锚点 | 待修 | B 族 |
| ISS-004 标签不透明背景 | 待修 | C 族 |
| ISS-005 共线 label 重叠 | 待修 | C 族 |
| ISS-009a 组内无连接节点横排 | 待修 | D 族 |
| cdn-cache 穿组=1 | 基线遗留 | 后续路由优化 |
| saas-schema 穿组=2 | 基线遗留 | 后续路由优化 |
| refund unnat=2（auto_approve→refund 等） | 软度量容留 | 后续评分优化 |
