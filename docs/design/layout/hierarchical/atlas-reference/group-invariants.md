# Group 不变量：containment / separation / penetration

> 父页：[atlas-reference/README.md](README.md)  
> 对应新架构：[architecture.md §6.3 三道防线 / §11 MetricVerifier](../architecture.md)  
> Atlas 源：`crates/v1/plotgram-core/src/layout/atlas/{group_invariant,channel/substrate}.rs`

## 这是什么

组的三条不变量：**containment**（组成员被组框包含）、**sibling separation**（兄弟组框不重叠）、**no penetration**（边路径不穿组框，只经 gate）。Atlas 的实现里，前两条在 `group_invariant.rs` 是 post-Ink 修复，第三条在 `substrate.rs` 是构建期 + L6 自反证。新架构 §6.3 要求「三道防线」由构造保证，§11 要求 MetricVerifier 独立复证。

## 三条不变量

| 不变量 | 含义 | Atlas 实现 | 位置 |
|--------|------|-----------|------|
| Containment | ∀ node n ∈ group g → n.rect ⊂ g.rect | 仅检测不修复（扩组框会引发 EdgeCrossesGroupInterior lint 回归） | `group_invariant.rs::report_containment_violations` |
| Sibling separation | ∀ 无祖先关系的组对 (g1, g2) → rect 不相交 | 沿 Main 轴（Y）推开下方组（刚体平移） | `group_invariant.rs::enforce_separation` |
| No penetration | 边路径不穿组框，只经 gate | 构建期防线（H2）+ L6 自反证 | `substrate.rs::verify_no_group_penetration` |

## Containment（仅检测不修复）

```rust
pub struct InvariantReport {
    pub containment_fixes: usize,   // 实际只是「检测数」，不修改几何
    pub separation_fixes: usize,
}
```

**Phase 1**：按 depth 降序 + id 排序遍历组，检测成员节点越界。**仅日志，不修改几何**。

注释原话：「`materialize()` 的 refine_group_frames LP 可能为 sibling separation 牺牲少量 containment；强行扩组框会引发 EdgeCrossesGroupInterior lint 回归，故仅报告」。

## Sibling separation（Y 推开）

**Phase 2** `enforce_separation`：

```
1. build_ancestry / build_member_map：构建后代集与成员映射
2. 按 (id_a, id_b) 排序遍历组对，跳过祖先关系对
3. rect_overlap_y / rect_overlap_x 都 > EPS 时：
   中心更靠下的组往下移 overlap_y + SIBLING_GAP
4. rigid_shift_group：
   平移组框（本组 + 后代组）+ 成员节点 + 落在组框矩形内的边路径点
   （按几何位置判定，跨组边仅框内段平移）
```

常量：`SIBLING_GAP = 8.0`，`EPS = 0.5`，`CONTAINMENT_THRESHOLD = 1.0`。

## No penetration（构建期 + L6 自反证）

### 构建期防线（H2）

`Substrate::link` 拒绝跨 scope 直连轨道 → `CrossScopeConnection`。这是「穿组不可表达」的根保证——边路径要在组内/组外转移，**必须经 gate**。

### L6 自反证

`Substrate::verify_no_group_penetration() -> Vec<PenetrationViolation>`：

```
对每段 × 每非祖先组，按 orient 选轴：
  Cross 段看 r0 < line ≤ r1 × [2·o0+1, 2·o1+1]
  Main 段对称
  line_inside && ext_overlaps → 违规
```

用奇坐标表达「内部」，故贴边界的组外段不误报。

**注意**：这是事后自反证，正确性来源是 derive 的构建期防线；L6 仅作回归探针。

## 三道防线（新架构 §6.3 对照）

| 防线 | Atlas 实现 | 新架构应做 |
|------|-----------|-----------|
| 第一道：构建期拒绝 | H2 穿组不可表达（Substrate::link） | 保留——Substrate 构建期拒绝 |
| 第二道：Metric 写组框 | `materialize()` refine_group_frames LP | 保留——Metric 相写组框，含 corridor demand |
| 第三道：Verifier 独立复证 | L6 自反证（substrate 内部）+ 测试硬断言 | 提升为 MetricVerifier 独立子检查，不复用构造代码 |

## 典型测试场景

| 测试名 | 验证什么 |
|--------|---------|
| `containment_detects_violation_without_modifying_geometry` | 检测到但不改几何 |
| `separation_pushes_overlapping_siblings_apart` | g2 推到 g1.bottom + SIBLING_GAP 之下，b 节点跟随移动 |
| `ancestor_groups_not_separated` | 祖先对不推开 |
| `no_groups_noop` | 空操作 |
| `l6_detects_injected_penetrating_segment` | 注入越界段报一条 |
| `b8_boundary_seam_belongs_to_outer_segment` | 边界缝归外段，组内段 link 边界缝被 scope 防线拒绝 |
| `derive_nested_groups_route_through_gate_chain` | 嵌套组穿出 ≥2 gate，L6 全过 |
| `product_flowchart_no_group_penetration` | L6 + verify_route_scope 硬断言无穿组 |
| `product_post_ink_no_group_penetration_lint` | post-Ink EdgeCrossesGroupInterior = 0 |

## 不该照搬

1. **Containment 只报告不修复**：不变量声明了却不强制，是「三路径壳」——宣称保护实际放弃。新实现应：要么真修（扩组框并重路由边），要么把 containment 提到上游保证（materialize LP 不应牺牲 containment）。
2. **`rigid_shift_group` 按几何位置判定边点归属**：跨组边的框内段被平移会导致**边路径轻微形变**（注释承认「轻微形变可接受」）。这是 post-Ink 几何修改，违反「下游不得推翻上游」。新实现应在 Metric 阶段保证组框不重叠，不在 post-Ink 推组。
3. **只沿 Y 推开**：对水平排列的兄弟组无效（X 方向重叠不修）。新实现应沿分离轴推开（或用 VPSC 在 Metric 阶段消除重叠）。
4. **验证逻辑分散**：`verify_no_group_penetration`（substrate）、`EdgeCrossesGroupInterior` lint（post-Ink）、`report_containment_violations`（group_invariant）三处。新架构 §11 应集中到 MetricVerifier。
5. **`product_flowchart_no_group_penetration` 测试同时断言 `verify_no_group_penetration` 与 `verify_route_scope`**：验证逻辑分散在 channel/substrate/plan 多处。新实现应集中到 MetricVerifier + InkVerifier，各相 verifier 内部保持独立证明器风格。

## 新实现建议

- **Containment 提到上游保证**：Metric 相的组框 LP 不应牺牲 containment；若必须牺牲，报 `InfeasibleConstraint` 走 relaxation ladder，不静默放弃。
- **Sibling separation 用 VPSC 在 Metric 阶段消除**，不在 post-Ink 推组。
- **No penetration 保留三道防线**：
  1. 构建期：Substrate::link 拒绝跨 scope 直连（H2）
  2. Metric 写组框：含 corridor demand，组框互不重叠
  3. MetricVerifier 独立复证：`verify_no_group_penetration` 不复用构造代码
- **InkVerifier 增 `EdgeCrossesGroupInterior` 检查**：post-Ink 验证边路径不穿组框（除了经 gate 的合法穿越），作为 InkVerifier 的 hard violation。
- **`SIBLING_GAP` / `CONTAINMENT_THRESHOLD` 作为 profile 参数**，不硬编码。
- **常量 `EPS = 0.5` 太粗**：新实现应基于 `TOL = CORRIDOR_LANE_PITCH` 或显式容差参数。
