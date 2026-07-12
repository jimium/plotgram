# LayoutLint Agent Guidance 升级实施计划

## Summary

基于 `docs/architecture/重构方案/layout-lint-agent-guidance-2026-07.md`，本次按 **全量 P0-P2** 落地 LayoutLint 升级：

1. 修正文档、指标、归因与现有实现不一致的问题，保持 lint 作为几何硬门禁的稳定性。
2. 在现有 `LayoutViolation` 之上新增 **LintAdvice** 指导层，把高价值违规映射到 `group_frame` / `group.layout` / `group_padding` 等 Agent 可操作旋钮。
3. 把 advice 能力接到 CLI、WASM 和 `agent-demo`，形成“validate -> lint --advice -> 改 DSL -> 再 lint/render”的闭环。

## Current State Analysis

### Core lint

- `crates/plotgram-core/src/layout/lint/violation.rs`
  - 当前 `LintReport` 只有 `violations`，没有建议层。
  - `LayoutViolation` 只有 `edge_index` 单值，无法完整表达 crossing / trunk merge 的双边归因。
- `crates/plotgram-core/src/layout/lint/config.rs`
  - 只有规则开关和 `fail_on_warning`，没有 advice 开关。
- `crates/plotgram-core/src/layout/lint/mod.rs`
  - `LayoutLinter::run()` 仍是逐条 `if cfg.is_enabled(...)` 硬编码串联。
  - `LintMetricsSummary` 仅覆盖 9 类规则，缺少 `node_outside_group`、`child_group_outside_parent`、`edge_on_group_border`。
  - `unrelated_edge_trunk_merge` 仍把边 index 塞进 `entity_ids`。
  - `sort_violations()` 只按 `rule/message/edge_index` 排序，区分度不足。

### Layout hints / group 接线

- `crates/plotgram-core/src/layout/mod.rs`
  - `LayoutHints` 已有 `group_layout_warnings`，但没有 `group_frame_report`。
- `crates/plotgram-core/src/layout/group_frame/pass.rs`
  - `apply_group_frame()` 的返回值被直接丢弃，没有写回 `LayoutResult.hints`。
- `crates/plotgram-core/src/layout/node/common/group_bounds.rs`
  - `GroupLayoutWarning` 已经在 Sugiyama 路径中生成，可作为 advice 的并行上下文。

### CLI / WASM / Agent

- `crates/plotgram-cli/src/main.rs`
  - `lint` 只有 `profile/ignore/fail-on-warning`，没有 `--advice`。
  - `validate --layout-check` 实际跑 strict，但帮助注释仍写“全量规则”。
- `crates/plotgram-wasm/src/lib.rs`
  - 当前仅暴露 `render` / `validate` / `parse_to_json` / `layout_catalog`，没有 `lint` 导出。
- `agent-demo/src/lib/wasm.ts`
  - TS 桥接层没有 lint 结果类型和调用封装。
- `agent-demo/src/agent/tools.ts`
  - Tool 列表只有 `render/validate/parse/diff/apply_patch/layout_catalog`。
- `agent-demo/src/agent/prompt.ts`
  - Prompt 要求“改后 validate 再 render”，但没有布局 advice 闭环，也没有 architecture 下 `group_frame` / `group layout` 的优先级指引。
- `agent-demo/src/components/ToolCallTrace.tsx`
  - 工具标签/主题表写死，新增 `lint` 需要同步更新。

### Docs

- `docs/guides/layout-lint.md`
  - 规则数、preset、strict 描述已落后于代码。
- `docs/guides/plotgram-cli.md`
  - 需要同步 `plotgram lint --advice` 和 `validate --layout-check` 的 strict 语义。
- `docs/product/agent-mcp-skills-strategy.md`
  - 工具体系中还没有 lint。

## Assumptions & Decisions

1. 本仓库无向后兼容约束，允许调整内部/对外 JSON 结构；但对 `LintReport` 保持“**advice 默认关闭，关闭时不输出 `advices`**”这一兼容性约束，以避免 CI / 现有消费者无谓噪音。
2. `LintAdvice` 采用 **report 顶层 `advices: Vec<LintAdvice>`**，而不是把建议嵌进 `LayoutViolation`，这样保持“事实层”和“建议层”分离，也便于 CLI/WASM/Agent 统一消费。
3. 边归因采用折中方案：
   - 保留 `edge_index: Option<usize>` 作为主索引，避免现有 CLI 文本输出重写过大。
   - 新增 `related_edge_indices: Vec<usize>`，用于 crossing / trunk merge 的完整归因。
4. `FixAction` 直接复用 `crates/plotgram-core/src/error.rs` 里的现有结构，不新增平行 fix 类型。
5. P2 的“MCP 接入”在本仓库内先落到 **文档与 agent-demo/wasm 工具面**；当前仓库里没有独立 MCP server 实现，因此本次不新增独立 MCP 进程代码。

## Proposed Changes

### 1. 扩展 lint 数据模型

#### 1.1 新增 advice 模块

- 新增文件：`crates/plotgram-core/src/layout/lint/advice.rs`
- 目标：
  - 定义 `LintAdvice`、`AdviceConfidence`、`LayoutKnob`。
  - 提供 `generate_lint_advices(diagram, result, violations, config) -> Vec<LintAdvice>`。
- 数据结构决策：
  - `LintAdvice` 字段：
    - `violation_index: usize`
    - `text: String`
    - `priority: u8`
    - `confidence: AdviceConfidence`
    - `knobs: Vec<LayoutKnob>`
    - `fix: Option<FixAction>`
  - `AdviceConfidence`: `High | Medium | Low`
  - `LayoutKnob` 覆盖：
    - `GroupFrame { field, suggested, rationale }`
    - `GroupLayout { group_id, suggested, rationale }`
    - `LayoutOption { key, suggested, rationale }`
    - `Topology { action, targets, rationale }`
    - `Label { edge_or_entity, action, rationale }`
    - `IgnoreRule { rule, rationale }`

#### 1.2 扩展 violation / report

- 修改：`crates/plotgram-core/src/layout/lint/violation.rs`
- 变更：
  - `LayoutViolation` 新增 `related_edge_indices: Vec<usize>`。
  - `LintReport` 新增 `advices: Vec<LintAdvice>`，并通过 serde `default + skip_serializing_if` 保持默认空输出。
  - 增加必要 helper：
    - `with_related_edges(...)`
    - `primary_edge_index()` 或保持现有 `edge_index` 语义不变。
- 原因：
  - 先解决归因模型不一致，再承载 advice 的索引映射。

#### 1.3 扩展 lint config

- 修改：`crates/plotgram-core/src/layout/lint/config.rs`
- 变更：
  - `LintConfig` 新增 `advice_enabled: bool`。
  - 新增 builder：`with_advice(bool) -> Self`。
  - 默认值：
    - `default/strict/verbose` 全部默认 `advice_enabled = false`。
- 原因：
  - 保证 CI / eval / 现有 JSON 调用方默认行为不变。

### 2. 重构 core lint 执行与归因

#### 2.1 接入 advice 生成

- 修改：`crates/plotgram-core/src/layout/lint/mod.rs`
- 变更：
  - `mod advice;` 并 re-export 新类型。
  - `LayoutLinter::run()` 在 `finalize_violations + sort_violations` 之后，按 `config.advice_enabled` 生成 `advices`。
  - 返回 `LintReport { violations, advices }`。

#### 2.2 修正 metrics 覆盖

- 修改：`crates/plotgram-core/src/layout/lint/mod.rs`
- 变更：
  - `LintMetricsSummary` 新增字段：
    - `node_outside_group`
    - `child_group_outside_parent`
    - `edge_on_group_border`
  - `from_report()` 完整覆盖 `LintRuleId::ALL`。
- 原因：
  - 对齐提案中的 P0 验收项，避免 total 与分项不一致。

#### 2.3 修正边归因

- 修改：`crates/plotgram-core/src/layout/lint/mod.rs`
- 变更：
  - `check_edge_crossings()`：
    - `edge_index` 写第一条边；
    - `related_edge_indices` 写两条边；
    - message 中保留双边信息。
  - `check_unrelated_edge_trunk_merge()`：
    - 不再把边 index 放进 `entity_ids`；
    - `edge_index + related_edge_indices` 表达两条边；
    - `entity_ids` 仅保留端点实体（若需要）。
- CLI 连带修改：
  - `print_lint_report_text()` 当 `related_edge_indices` 非空时打印 `edge_indices: ...`。

#### 2.4 改进排序与微性能

- 修改：`crates/plotgram-core/src/layout/lint/mod.rs`
- 变更：
  - `sort_violations()` 排序键改为：
    - `rule`
    - `severity`
    - `message`
    - `group_ids`
    - `entity_ids`
    - `edge_index`
    - `related_edge_indices`
  - 对 `check_edge_*` 内部重复 `sort()` 的 group id 收集做外提或局部缓存，避免循环内反复排序。

### 3. Advice 规则映射实现

#### 3.1 首批高价值规则

- 文件：`crates/plotgram-core/src/layout/lint/advice.rs`
- 必做映射：
  - `group_overlap`
    - 高优先级：`group_frame.gap += delta`
    - 次优先级：`group_frame.track = fit/equal`
    - 低优先级：`Topology(split_group/check_topology)`
  - `node_outside_group`
    - 高优先级：`layout.group_padding = excess + margin`
    - 次优先级：目标 group 改 `layout`
  - `child_group_outside_parent`
    - 中优先级：父 group `group_padding`
    - 次优先级：子 group `layout`
  - `sibling_width_ratio`
    - architecture 下优先 `group_frame: strips` / `track: equal`
    - 如果 `group_frame_report` 已显示 `equalized=true`，则避免重复推荐 equal，转为 `fit` 或组内 layout 调整
  - `edge_on_group_border`
    - 输出 `IgnoreRule`

#### 3.2 次级规则

- 同文件追加 text-only / low-confidence 建议：
  - `edge_crosses_group_interior`
  - `node_overlap`
  - `edge_through_node`
  - `edge_crossing`
  - `label_node_overlap`
  - `label_label_overlap`
  - `unrelated_edge_trunk_merge`
- 这些规则默认不给自动 `fix`，只给说明与 `knobs`。

#### 3.3 fix 白名单

- 同文件实现安全 fix 生成，仅对 `confidence=high` 输出：
  - `set_group_frame_preset`
  - `set_group_frame_field`
  - `set_layout_option`
- 本次明确 **不** 自动生成：
  - 拆组/改拓扑
  - 移节点到其他 group
  - 任意文案缩写

### 4. 接入 GroupFrame / LayoutHints 上下文

#### 4.1 把 GroupFrameReport 写回 LayoutHints

- 修改：`crates/plotgram-core/src/layout/mod.rs`
- 变更：
  - `LayoutHints` 新增 `group_frame_report: Option<group_frame::GroupFrameReport>`。

#### 4.2 GroupFramePass 写入 hints

- 修改：`crates/plotgram-core/src/layout/group_frame/pass.rs`
- 变更：
  - 在 `apply_after_node_snap()` 与 `restore_after_node_moves()` 中捕获 `apply_group_frame()` 返回值，写入 `layout.hints.group_frame_report`。
  - 若同一轮多次调用，以最后一次为准。

#### 4.3 advice 使用 hints

- 修改：`crates/plotgram-core/src/layout/lint/advice.rs`
- 变更：
  - 读取 `result.hints.group_frame_report` 和 `result.hints.group_layout_warnings`。
  - 用途：
    - 避免重复推荐已生效的 `equal/strips`。
    - 让 group warning 与 lint advice 至少在说明层统一，不再完全双轨。

### 5. CLI 升级

#### 5.1 `plotgram lint --advice`

- 修改：`crates/plotgram-cli/src/main.rs`
- 变更：
  - `Lint` 子命令新增 `--advice` 布尔开关。
  - `build_lint_config()` 接收 `advice` 参数并设置 `with_advice(true)`。
  - `print_lint_report_text()`：
    - 先输出 violation；
    - 对应的 advice 逐条缩进打印 `advice(priority/confidence): ...`；
    - 如存在 `fix`，补充 `fix.action` 摘要。
  - JSON 模式直接输出带 `advices` 的 `LintReport`。

#### 5.2 修正文案

- 同文件同步：
  - `Validate.layout_check` 的注释改为“额外执行 LayoutLint strict 预设”。

### 6. WASM 导出与 Agent Demo 接入

#### 6.1 新增 wasm lint API

- 修改：`crates/plotgram-wasm/src/lib.rs`
- 变更：
  - 新增：
    - `LintResultJson { report, success }` 或直接返回 `LintReport` 外加 `success/acceptable` 包装；
    - `WasmLintOptions { profile, fail_on_warning, advice }`
  - 导出：
    - `lint(source: &str) -> String`，默认 `profile=default, advice=true`
    - `lint_with_options(source: &str, options_json: &str) -> String`
  - 内部流程：
    - `parse_prepare_validate`
    - 语法语义失败时返回错误结构，不尝试 layout
    - 成功时跑 `LayoutLinter`
- 决策：
  - WASM 默认开 advice，因为它面向 Agent/UI，不面向 CI。

#### 6.2 TS wasm bridge

- 修改：`agent-demo/src/lib/wasm.ts`
- 变更：
  - 扩充 `PlotgramWasm` 接口：`lint` / `lint_with_options`
  - 新增类型：
    - `LintSeverityJson`
    - `LintViolation`
    - `LintAdvice`
    - `LintResult`
    - `LintOptions`
  - 新增 helper：`lintSource(wasm, source, options?)`

#### 6.3 Agent 工具面

- 修改：`agent-demo/src/agent/tools.ts`
- 变更：
  - 在 `AGENT_TOOL_SCHEMAS` 中新增 `lint` tool：
    - 入参：`source`、可选 `profile`、可选 `advice`
    - 返回：结构化 violations + advices
  - 在 `createToolExecutors()` 中新增执行器，默认 advice=true。

- 修改：`agent-demo/src/components/ToolCallTrace.tsx`
- 变更：
  - 为 `lint` 补标签、配色和 icon。

- 修改：`agent-demo/src/hooks/useAgent.ts`
- 变更：
  - 更新工具参数/result 的预览摘要逻辑，避免 `lint` 返回大 JSON 时完全刷满轨迹面板。

#### 6.4 Prompt 升级

- 修改：`agent-demo/src/agent/prompt.ts`
- 变更：
  - 工作流调整为：
    - 修改 DSL 后先 `validate`
    - 若 validate 通过，再 `lint(advice=true)`
    - 有 error 时优先按 advice 调 `group_frame` / `group.layout` / `group_padding`
    - warning 不要求清零
  - architecture 知识模块追加：
    - containment / overlap / sibling 宽比的 advice 处理顺序
    - `edge_on_group_border` 可忽略

### 7. 文档同步

#### 7.1 LayoutLint 指南

- 修改：`docs/guides/layout-lint.md`
- 变更：
  - 更新规则总数为 12。
  - 更新 preset/strict 行为为 7 条硬规则。
  - 增加 `--advice`、`LintAdvice`、Agent 工作流、JSON 输出示例。

#### 7.2 CLI 指南

- 修改：`docs/guides/plotgram-cli.md`
- 变更：
  - 补充 `plotgram lint --advice`
  - 明确 `validate --layout-check` 等价 strict。

#### 7.3 MCP / Agent 产品文档

- 修改：`docs/product/agent-mcp-skills-strategy.md`
- 变更：
  - 在 tool 矩阵中加入 `lint`。
  - 在 Agent 工作流中加入 `validate -> lint(advice) -> render`。

## Implementation Order

1. **P0 基线修正**
   - 先改 `violation.rs` / `config.rs` / `lint/mod.rs`
   - 补齐 `LintMetricsSummary`
   - 修正归因与排序
2. **P1 advice MVP**
   - 新增 `lint/advice.rs`
   - 接入 `LintReport.advices`
   - 打通 `GroupFrameReport -> LayoutHints`
   - CLI `--advice`
3. **P2 surface**
   - `plotgram-wasm` 暴露 `lint`
   - `agent-demo` tool / prompt / trace
   - 文档统一更新

## Verification Steps

### Rust tests

1. `cargo test -p plotgram-core layout::lint`
2. `cargo test -p plotgram-core group_frame`
3. `cargo test -p plotgram-cli`
4. `cargo test -p plotgram-wasm`

### CLI smoke

1. `cargo run -p plotgram-cli -- lint showcase/architecture/c.k8s-multi-namespace-overview.pgm --format json`
2. `cargo run -p plotgram-cli -- lint showcase/architecture/c.k8s-multi-namespace-overview.pgm --advice --format json`
3. `cargo run -p plotgram-cli -- validate showcase/architecture/c.k8s-multi-namespace-overview.pgm --layout-check`

### Advice acceptance

1. `sibling_width_ratio` 用例返回 `group_frame: strips` 或 `track: equal` advice。
2. `node_outside_group` / `child_group_outside_parent` advice 包含 `group_padding`，且建议值不小于 excess + margin。
3. `edge_on_group_border` advice 输出 `IgnoreRule`，不误导增加 gap。
4. `--advice` 关闭时 JSON 中不输出 `advices` 字段。

### Frontend / Agent smoke

1. `npm --prefix agent-demo run build`
2. 在 agent-demo 中确认工具列表出现 `lint`。
3. 用 architecture 示例验证：`validate -> lint -> render` 链路能返回 advice，且 tool trace 能正确展示摘要。

## Risks & Mitigations

- **建议误导 Agent**：通过 `confidence` 分级与 fix 白名单降低风险；低置信只给 text/knob。
- **JSON 结构膨胀影响现有消费者**：默认关闭 advice，空 `advices` 不序列化。
- **P2 改动分散**：严格按“core -> CLI -> wasm -> agent-demo -> docs”顺序推进，每一层先保持最小闭环再向上接线。
- **Group hint 双轨仍混乱**：本次至少把 `group_frame_report` 写回 hints，并在 advice 层消费；`group_layout_warnings` 暂不直接并入 `LintReport.violations`，避免一次性重写太多检测逻辑。
