# 布局方向（`direction`）重构方案

> 本文档定义 `diagram.direction` 与节点布局算法之间的契约重构方案。
> 目标：消除全局 fallback、算法内特殊分支与静默忽略，使方向能力与边路由
> `applicable_diagram_types` 模式对齐——**算法声明能力，组合不合法则 fail fast**。
>
> **状态**：设计定稿，待实施  
> **范围**：`drawify-core`（profile / layout / ast / validation）、`layout/catalog`、Playground UI  
> **关联**：`docs/specs/dsl/language-spec.md`、`crates/drawify-core/src/profile/mod.rs`

---

## 1. 背景与问题

### 1.1 现状

| 层级 | 当前行为 | 问题 |
|------|----------|------|
| DSL | `direction` 全局三值枚举（`top-to-bottom` / `left-to-right` / `radial`） | 与具体 `layout` 无交叉校验 |
| `Diagram::direction()` | AST 无属性时硬编码 `top-to-bottom` | 全局 fallback，与 mindmap 实际布局不一致 |
| `mindmap` 算法 | `layout_mode()` 检测「是否写了 direction」；未写则强制 `radial` | 算法内特殊化，绕过统一路径 |
| Sugiyama 系列 | 读 `diagram.direction()`，仅识别 `left-to-right` vs 其他 | `radial` 等值被静默当作 `top-to-bottom` |
| `circular` / `sequence` / `architecture` / `force-directed` | 不读 `direction` | 用户写了 `direction` 以为生效，实际无效果 |

### 1.2 设计矛盾

`direction` 同时承担三种角色，但没有协调层：

1. **DSL 图级属性**（用户可见的配置项）
2. **布局算法运行参数**（只有部分算法消费）
3. **产品默认值**（mindmap 默认 radial，文档却写 top-to-bottom）

### 1.3 已定决策

1. **L1–L3 分层**：算法声明能力 → profile 提供图类型默认 → 统一解析入口。
2. **mindmap 不特殊化**：删除 `has_explicit_direction` 分支，与其他算法走同一 resolver。
3. **严格校验**：`effective_direction` 不被当前 `layout` 支持 → **报错**，给出可操作提示。
4. **按图类型默认方向**：`DiagramProfile` 增加 `default_direction: Option<&'static str>`，**去掉** `ast.rs` 全局 `top-to-bottom` fallback。
5. **不参与 direction 的图类型**：profile 中 `default_direction = None`（如 `sequence`）。

---

## 2. 设计原则

1. **算法是方向能力的权威**：`LayoutStrategy::supported_directions()` 声明合法取值。
2. **Profile 是图类型默认的权威**：与 `default_layout` / `default_edge_routing` 对称；仅用于「用户未写 `direction`」时的默认值，不参与运行时算法选择。
3. **单一解析入口**：布局、grid snap、校验、catalog 均通过 `resolve_effective_direction()` 获取有效方向，禁止各算法自行判断「AST 里有没有 direction 属性」。
4. **Fail fast**：不静默忽略、不隐式回退；错误信息须包含当前值、支持列表、建议操作。
5. **无向后兼容约束**（见 `AGENTS.md`）：可删除 mindmap 特殊逻辑，可对以往静默接受的非法组合报错。

---

## 3. 三层架构

```
┌─────────────────────────────────────────────────────────────┐
│  L2  DiagramProfile                                         │
│  default_direction: Option<&'static str>  （按 diagram type）│
└──────────────────────────┬──────────────────────────────────┘
                           │ 用户未写 direction 时
                           ▼
┌─────────────────────────────────────────────────────────────┐
│  L3  resolve_effective_direction(diagram)                   │
│  = AST 显式 direction ?? profile.default_direction          │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│  L1  LayoutStrategy::supported_directions()                 │
│  校验 effective ∈ supported；否则 DiagnosticError             │
└─────────────────────────────────────────────────────────────┘
```

### 3.1 L1 — 布局算法声明能力

在 `LayoutStrategy` trait（`layout/mod.rs`）上新增：

```rust
/// 该布局算法支持的方向列表。
/// 空切片表示不消费 diagram 级 `direction`。
fn supported_directions(&self) -> &'static [&'static str] {
    &[]
}
```

各算法在自身模块内以 `const SUPPORTED_DIRECTIONS` 覆写，模式与边路由 `APPLICABLE_TYPES` 一致。

**不要求**在算法上再声明 `default_direction()`——图类型默认统一由 profile 管理，避免「换 layout 默认值跟着变」的歧义。

### 3.2 L2 — 图类型 Profile 默认

在 `DiagramProfile`（`profile/mod.rs`）上新增：

```rust
/// 用户未在 DSL 中声明 `direction` 时的默认值。
/// `None` 表示该图类型不参与 direction 体系（布局默认算法不消费 direction）。
pub default_direction: Option<&'static str>,
```

`custom_profile` 继承 `FLOWCHART_PROFILE.default_direction`（与 `default_layout` 一致）。

> **设计决策：`default_direction` 绑定 diagram type 而非 layout 算法**
>
> `default_direction` 是 diagram type 级别的属性，与 `default_layout` / `default_edge_routing` 对称。
> 这意味着当用户显式切换 layout（如 `diagram flowchart { layout: mindmap }`）但未写 `direction` 时，
> effective direction 仍取 flowchart profile 的 `top-to-bottom`，而非 mindmap 的 `radial`。
>
> 理由：
> 1. **一致性**：`default_layout` 和 `default_edge_routing` 同样绑定 diagram type，direction 应保持对称。
> 2. **可预测性**：用户声明 `diagram flowchart` 即意味着 flowchart 语义，切换 layout 不应隐式改变方向默认值。
> 3. **交叉校验兜底**：若 flowchart profile 默认 `top-to-bottom` 与切换后的 mindmap layout 不兼容，
>    §4.2 的交叉校验会报错，用户需显式声明 `direction`——这比静默切换更安全。
> 4. **Playground 可辅助**：UI 层在用户切换 layout 时可检测 direction 不兼容并提示调整，
>    但 Rust 侧校验始终是权威。

### 3.3 L3 — 统一解析

新增模块级函数（建议放在 `layout/direction.rs` 或 `layout/mod.rs`）：

```rust
/// 解析 diagram 的有效布局方向。
///
/// - 若 AST 显式声明 `direction` → 返回 `Some(该值)`
/// - 否则若 profile.default_direction 为 `Some` → 返回 profile 默认
/// - 否则 → `None`（该图不参与 direction 体系）
pub fn resolve_effective_direction(diagram: &Diagram) -> Option<&'static str> {
    if let Some(explicit) = diagram.direction_attr() {
        return Some(explicit); // 见 §4.2 关于生命周期的处理
    }
    profile_for(&diagram.diagram_type).default_direction
}
```

`Diagram::direction()` 改为委托 `resolve_effective_direction()`，或拆为：

- `direction_attr()` → 仅 AST 显式值（`Option<&str>`）
- `effective_direction()` → 走 resolver（`Option<&str>`）

推荐保留 `direction()` 作为 **effective** 的别名并更新文档；所有布局代码统一调用 resolver。

---

## 4. 校验规则

校验在 `validate_layout_config()`（`layout/mod.rs`）中执行，位于 `compute_layout` 之前。

### 4.1 `direction` 属性值合法性

保留全局方向**词汇表**校验（合法 atom 值），与 profile / layout 无关：

```rust
const VALID_DIRECTION_ATOMS: &[&str] = &[
    "top-to-bottom",
    "left-to-right",
    "radial",
];
```

> **注意**：`from_center` 在旧版 `VALID_LAYOUT_DIRECTIONS` 中曾作为 `radial` 的别名存在，本次重构**移除**该别名。DSL 不再接受 `from_center`，统一使用 `radial`。

### 4.2 `direction` × `layout` 交叉校验

设：

- `algo` = 当前 diagram 解析得到的 layout 算法名（含 profile 默认）
- `strategy` = 对应 `LayoutStrategy` 实例
- `effective` = `resolve_effective_direction(diagram)`
- `supported` = `strategy.supported_directions()`

| 条件 | 行为 |
|------|------|
| `supported.is_empty()` 且 AST **有**显式 `direction` | **报错**：「`layout: {algo}` 不支持 `direction`，请删除该属性」 |
| `supported.is_empty()` 且 AST **无**显式 `direction` | 通过（direction 不参与该 layout 的体系，无需校验） |
| `supported.non_empty()` 且 `effective` 为 `None` | **报错**：「diagram type `{name}` 未配置默认 direction」——内置 profile 不应触发 |
| `supported.non_empty()` 且 `effective ∉ supported` | **报错**：见 §5 |
| `supported.non_empty()` 且 `effective ∈ supported` | 通过 |

> **校验伪代码**：
>
> ```rust
> fn validate_direction_layout_compat(diagram: &Diagram, strategy: &dyn LayoutStrategy) -> Result<()> {
>     let supported = strategy.supported_directions();
>     let explicit = diagram.direction_attr(); // 仅 AST 显式值
>
>     if supported.is_empty() {
>         // layout 不消费 direction
>         if explicit.is_some() {
>             return Err(direction_not_supported_error(explicit, strategy.name()));
>         }
>         return Ok(()); // 无需校验，不看 effective
>     }
>
>     // supported 非空：layout 消费 direction
>     let effective = resolve_effective_direction(diagram)
>         .ok_or_else(|| missing_default_direction_error(diagram.diagram_type()))?;
>
>     if !supported.contains(&effective) {
>         return Err(direction_layout_mismatch_error(effective, strategy.name(), supported));
>     }
>     Ok(())
> }
> ```

### 4.3 与 `group_arrangement` 的关系

`group_arrangement`（`vertical` / `horizontal`）是 **flowchart 含 group 时组间宏观排列**的独立属性，不并入 `direction` 枚举，不参与本方案的 `supported_directions` 校验。

---

## 5. 错误信息规范

沿用 `DiagnosticError::invalid_enum_value` 风格，扩展上下文字段。

### 5.1 direction 值不在全局词汇表

```
invalid value for 'direction': "diagonal"
expected one of: top-to-bottom, left-to-right, radial
```

### 5.2 layout 不支持当前 effective direction

```
layout 'sugiyama-v2' does not support direction 'radial'.
Supported directions: top-to-bottom, left-to-right.
Hint: set direction: top-to-bottom, or use layout: mindmap.
```

### 5.3 layout 不消费 direction 但用户写了

```
layout 'sequence' does not support the 'direction' attribute.
Remove 'direction' from the diagram block.
```

### 5.4 实现建议

新增辅助函数，例如：

```rust
fn direction_layout_mismatch_error(
    span: Span,
    layout: &str,
    direction: &str,
    supported: &[&str],
    diagram_type: &str,
) -> DiagnosticError
```

`Hint` 行可根据 `applicable_layouts_for_type` + 各 layout 的 `supported_directions` 自动生成一条最相关建议。

---

## 6. Profile 默认值表

内置图类型的 `default_direction` 初始配置：

| DiagramType | `default_direction` | 说明 |
|-------------|---------------------|------|
| `Flowchart` | `Some("top-to-bottom")` | 默认 `layout: flowchart`（Sugiyama） |
| `Architecture` | `Some("top-to-bottom")` | |
| `Er` | `Some("top-to-bottom")` | |
| `Mindmap` | `Some("radial")` | 替代算法内隐式默认 |
| `State` | `None` | 默认 `layout: state`（circular）不消费 direction |
| `Sequence` | `None` | 默认 `layout: sequence` 不消费 direction |
| `Custom` | 继承 Flowchart → `Some("top-to-bottom")` | |

> **注意**：`State` / `Sequence` 的 `None` 不代表用户不能写 `direction`——若用户显式写了，仍须通过 §4.2 第一条报错（因默认 layout 不消费）。若未来 state 支持可切换至 Sugiyama 类 layout，profile 默认可改为 `Some("top-to-bottom")` 并在切换 layout 时由 Playground 提示调整 direction。

---

## 7. 布局算法 `supported_directions` 表

| 算法名 | `supported_directions` | 消费位置 |
|--------|------------------------|----------|
| `sugiyama` | `top-to-bottom`, `left-to-right` | `backup/sugiyama.rs` |
| `sugiyama-v2` | `top-to-bottom`, `left-to-right` | `sugiyama_v2/engine.rs` |
| `flowchart` | `top-to-bottom`, `left-to-right` | 共享 sugiyama_v2 engine；含 group 时 `group_divide` 组内仍用 Sugiyama |
| `er` | `top-to-bottom`, `left-to-right` | 共享 sugiyama_v2 engine |
| `mindmap` | `radial`, `top-to-bottom`, `left-to-right` | `mindmap.rs` |

> **关于 `MindmapMode::TrueRadial`**：当前 mindmap 内部有 `Radial`（水平双向）和 `TrueRadial`（极坐标径向）
> 两种变体，但 `TrueRadial` 是运行时根据 root 子节点数（≥ 3）自动升级的，不是用户可配置的。
> 因此 `supported_directions` 只声明 `radial`，`layout_mode()` 将 `radial` 映射为 `MindmapMode::Radial`，
> 运行时升级为 `TrueRadial` 的逻辑不变。

| `architecture` | `[]` | 不读 direction（v2 自有分层逻辑） |
| `force-directed` | `[]` | |
| `circular` | `[]` | |
| `state` | `[]` | 门面，共享 circular 引擎 |
| `sequence` | `[]` | 边几何由布局内置 |

> **关于 `from_center` 的移除**：旧版代码中 `from_center` 作为 `radial` 的别名存在于 `VALID_LAYOUT_DIRECTIONS`，但 `attr_constants::direction::ALL` 并未包含它，两处定义不同步。本次重构统一移除 `from_center`，DSL 仅接受 `radial`，消除别名带来的归一化复杂度。

---

## 8. 代码变更清单

### 8.1 `profile/mod.rs`

- [ ] `DiagramProfile` 增加 `default_direction: Option<&'static str>`
- [ ] 各 `*_PROFILE` 静态实例填入 §6 表格
- [ ] `custom_profile` 继承 flowchart
- [ ] 单元测试：每个 builtin profile 的 `default_direction` 与 §6 一致

### 8.2 `layout/mod.rs` — `LayoutStrategy` trait

- [ ] 新增 `supported_directions()`
- [ ] 各 `impl LayoutStrategy` 覆写（见 §7）
- [ ] 新增 `resolve_effective_direction()` / 导出
- [ ] 重写 `validate_layout_config()` 交叉校验（§4）
- [ ] 删除 `VALID_LAYOUT_DIRECTIONS` 的单一用途，拆为词汇表 + 交叉校验
- [ ] 从 `VALID_LAYOUT_DIRECTIONS` / `attr_constants::direction` 中移除 `from_center`，统一为三值枚举

### 8.3 `ast.rs`

- [ ] 删除 `direction()` 中的 `TOP_TO_BOTTOM` 硬编码 fallback
- [ ] 新增 `direction_attr() -> Option<&str>`（仅显式 AST）
- [ ] `direction()` 或 `effective_direction()` 委托 resolver

### 8.4 `layout/node/mindmap.rs`

- [ ] **删除** `layout_mode()` 中 `has_explicit_direction` 特殊分支
- [ ] `layout_mode()` 仅根据 `resolve_effective_direction()` 映射 `MindmapMode`
- [ ] 修正错误注释（「显式声明 layout」→「显式或 profile 默认 direction」）

### 8.5 其他布局消费点

- [ ] `sugiyama_v2/engine.rs`：`horizontal` 判断改读 effective direction
- [ ] `layout/mod.rs` grid snap：`horizontal` 改读 effective direction
- [ ] `backup/sugiyama.rs`：同上（若仍保留）

> **radial 方向与 grid snap**：当前 grid snap 仅区分 `horizontal: bool`，对 `radial` 方向
> 会落入 `horizontal = false`（按垂直方向 snap），这在 mindmap 的 radial 布局下语义不正确。
> 处理策略：**radial 方向下跳过 grid snap**。理由：
> 1. radial 布局的节点分布不是沿 rank/layer 轴的网格，grid snap 的对齐假设不成立。
> 2. mindmap 算法自身已处理节点间距，无需后置 snap 修正。
> 3. 实现方式：在 `compute_layout()` 的 grid snap 分支中，当 `effective_direction == "radial"` 时
>    直接跳过 snap，不设置 `horizontal` 标志。

### 8.6 `layout/catalog.rs`

- [ ] `DiagramTypeCatalog` 增加 `default_direction: Option<String>`
- [ ] `LayoutAlgoInfo` 可选增加 `supported_directions: Vec<String>`（供 Playground 过滤）
- [ ] WASM 导出字段同步

### 8.7 Playground（`playground/src/data/layoutOptions.ts`）

- [ ] `buildLayoutDirectionOptions()` 按当前 `layoutAlgo` 的 `supported_directions` 过滤
- [ ] 新建图 / 切换 diagram type 时，默认 direction 从 catalog 的 `default_direction` 读取
- [ ] 切换 layout 时，若当前 direction 不被支持 → UI 提示或自动回退到该 layout 支持的第一项（可选；Rust 侧校验仍为权威）

### 8.8 文档

- [ ] `docs/specs/dsl/language-spec.md`：`direction` 默认值改为「由图表类型 profile 决定」
- [ ] `docs/specs/dsl/dsl-writing-manual.md` 示例同步
- [ ] 各 `docs/specs/visual-language/diagrams/*.md` 中 mindmap / flowchart 相关说明

---

## 9. 数据流（布局运行时）

```
DSL parse
    → Diagram AST
    → validate_layout_config()
         ├─ direction atom 合法？
         ├─ layout 已注册且 supports diagram_type？
         └─ effective_direction × layout.supported_directions？
    → compute_layout()
         ├─ resolve_effective_direction(diagram)
         ├─ strategy.compute(diagram)   // 内部读 effective，非 AST 特殊判断
         ├─ grid_snap (读 effective 判断 horizontal)
         └─ edge routing ...
```

---

## 10. 破坏性变更与迁移

| 以往行为 | 重构后 |
|----------|--------|
| `diagram flowchart { direction: radial }` 静默当 tb | **报错** |
| `diagram mindmap` 无 direction，布局 radial，`direction()` 返回 tb | **统一**：effective = profile `radial` |
| `diagram sequence { direction: top-to-bottom }` 静默忽略 | **报错**（删除 direction） |
| `direction: from_center` 被接受（别名） | **报错**（`from_center` 不再是合法值，使用 `radial`） |
| 全局默认 tb | **移除**；各图类型查 profile |

无需保留 deprecated 或兼容层（`AGENTS.md` §1）。

---

## 11. 实施阶段

### Phase 1 — 契约与解析（core）

1. Profile 字段 + 各图默认值
2. `LayoutStrategy::supported_directions()` 各算法实现
3. `resolve_effective_direction()` + AST 改造：
   - 新增 `direction_attr() -> Option<&str>`
   - **暂不修改** `direction()` 的行为（仍返回硬编码 `top-to-bottom`），避免在交叉校验上线前引入行为变化
4. 单元测试：resolver、profile 默认值

> **Phase 1 的自洽性**：此阶段 `direction()` 仍走旧路径，`resolve_effective_direction()` 作为新入口
> 仅被新增代码调用。mindmap 的 `layout_mode()` 仍走 `has_explicit_direction` 分支。
> 旧路径与新路径并存但互不干扰，确保 Phase 1 可独立提交。

### Phase 2 — 校验与报错

1. `validate_layout_config` 交叉校验
2. 错误信息辅助函数 + 诊断测试
3. 删除 mindmap 特殊分支
4. **切换 `direction()` 到新路径**：`direction()` 委托 `resolve_effective_direction()`，删除硬编码 fallback
5. 移除 `from_center`：从 `VALID_LAYOUT_DIRECTIONS` 和 `attr_constants::direction` 中删除

> **Phase 2 的行为切换点**：步骤 3-4 是本次重构的行为变更核心。删除 mindmap 特殊分支后，
> `layout_mode()` 改读 `resolve_effective_direction()`；`direction()` 切换后所有消费方统一走 resolver。
> 这两步应在同一 commit 中完成，避免中间态不一致。

### Phase 3 — 消费方统一

1. sugiyama engine / grid snap 改读 resolver  
2. 全量 `layout::` 测试修复

### Phase 4 — 对外暴露

1. `layout/catalog` 扩展  
2. Playground direction 下拉与切换逻辑  
3. language-spec 文档更新

---

## 12. 测试计划

### 12.1 单元测试

| 用例 | 期望 |
|------|------|
| mindmap 无 direction | effective = `radial` |
| flowchart 无 direction | effective = `top-to-bottom` |
| sequence 无 direction | effective = `None` |
| flowchart + `direction: radial` + `layout: flowchart` | validate 失败 |
| mindmap + `direction: radial` + `layout: mindmap` | validate 通过 |
| sequence + 显式 `direction: top-to-bottom` | validate 失败 |
| custom diagram 无 direction | effective = flowchart 默认 |

### 12.2 回归

- 现有 mindmap / flowchart / er 黄金样例在补全或依赖 profile 默认后仍通过
- `drawify-eval` 若有 direction 相关用例一并更新

---

## 13. 不在本方案范围内

- `group_arrangement` / `group_gap` / `group_align`（flowchart 分治专属）  
- 边路由与 direction 的关系（正交路由的 horizontal snap 仅间接依赖 effective direction）  
- 将 mindmap 的 `radial` 迁入 `layout: mindmap { mode: … }` 配置块（长期可选，非本次）

---

## 14. 参考：与边路由模式的对照

| 维度 | 边路由（已落地） | 布局方向（本方案） |
|------|------------------|-------------------|
| 能力声明 | `EdgeRoutingStrategy::applicable_diagram_types()` | `LayoutStrategy::supported_directions()` |
| 图类型默认 | `DiagramProfile::default_edge_routing` | `DiagramProfile::default_direction` |
| 交叉校验 | layout × diagram_type、edge_routing × diagram_type | direction × layout |
| 解析入口 | `LayoutPlan` / `compute_layout` | `resolve_effective_direction()` |
| 不参与体系 | 时序图 `uses_edge_routing: false` | sequence `default_direction: None` + `supported_directions: []` |

---

## 15. 变更记录

| 日期 | 说明 |
|------|------|
| 2026-06-23 | 初稿：L1–L3、profile `Option` 默认、严格校验、mindmap 去特殊化 |
| 2026-06-23 | 修订：移除 `from_center` 别名；补充 `default_direction` 绑定 diagram type 论证；补充 radial + grid snap 策略；明确校验伪代码；明确 Phase 1/2 交界行为；补充 `TrueRadial` 映射说明 |
