# LayoutLint 复盘与 Agent 指导能力方案

> 日期：2026-07-12  
> 状态：提案（未实现）  
> 相关：[`docs/guides/layout-lint.md`](../../guides/layout-lint.md)、[`docs/guides/group-layout-and-frame.md`](../../guides/group-layout-and-frame.md)、[`docs/specs/error-model.md`](../../specs/error-model.md)、[`docs/product/agent-mcp-skills-strategy.md`](../../product/agent-mcp-skills-strategy.md)

---

## 0. 一句话结论

LayoutLint 作为**几何硬门禁**已经够用（确定性、可归因、CLI/eval 可消费），但离「面向 Agent 的布局辅导」还差一层：**机器可读的修复建议（尤其 group hint / group_frame）**。  
建议分两步走——先补齐规则/指标/归因一致性（P0），再引入与 `DiagnosticError.suggestion` 对齐的 **LintAdvice**，把违规映射到 L1/L2/L3 DSL 旋钮（P1），最后接到 Studio/MCP 工具面（P2）。

---

## 1. 现状复盘

### 1.1 设计定位（做对了什么）

| 设计点 | 评价 |
|--------|------|
| 布局后几何检查，不解析 SVG | 正确：与渲染解耦，确定性好 |
| 违规可追溯到 entity / group / edge | 正确：Agent / CI 都能定位 |
| error vs warning + Profile（default / strict / verbose） | 正确：日常与门禁分层 |
| 与 `plotgram-eval` 分工（逐条 vs 聚合） | 正确：职责清晰 |
| 合法穿越豁免（祖先 group、父子重叠） | 正确：减少假阳性 |

数据流：

```text
.pgm → prepare → compute_layout → LayoutResult
                                      ↓
                                LayoutLinter::run
                                      ↓
                                   LintReport
```

实现：`crates/plotgram-core/src/layout/lint/`（`mod.rs` / `violation.rs` / `config.rs` / `geometry.rs`）。

### 1.2 当前能力清单（代码为准）

**12 条规则**（文档仍写 8 条，已滞后）：

| 规则 | 默认级别 | strict | 主要归因 |
|------|----------|--------|----------|
| `node_overlap` | error | ✓ | entity_ids |
| `group_overlap` | error | ✓ | group_ids |
| `node_outside_group` | error | ✓ | entity + group + metric |
| `child_group_outside_parent` | error | ✓ | group_ids + metric |
| `edge_through_node` | error | ✓ | entities + edge_index |
| `edge_crosses_group_interior` | error | ✓ | edge + groups |
| `label_node_overlap` | error | ✓ | entities |
| `edge_crossing` | warning | ✗ | edge_index（仅一条） |
| `edge_on_group_border` | warning | ✗ | edge + groups（default 关闭） |
| `unrelated_edge_trunk_merge` | warning | ✗ | 边 index 塞进 entity_ids |
| `label_label_overlap` | warning | ✗ | entities |
| `sibling_width_ratio` | warning | ✗ | group_ids + ratio metric |

**消费面**：

- CLI：`plotgram lint`、`validate --layout-check`（实际跑 strict）
- Eval：`LintMetricsSummary` → baseline
- 算法测试：`post_layout.rs` 等用 lint 做回归断言
- **Studio Agent / WASM / MCP：无 lint 工具**；prompt 未提 group_frame / group layout

### 1.3 与错误模型的断层

[`error-model.md`](../../specs/error-model.md) 已有成熟的 Agent 闭环：

```text
DiagnosticError → suggestion.text + suggestion.fix → Agent 改 DSL → 重试
```

LayoutLint 只有「哪里错了」，没有「怎么修」：

| | DiagnosticError | LayoutViolation |
|--|-----------------|-----------------|
| 位置 | source Location | 无源码 span（布局几何） |
| 建议 | `Suggestion { text, fix }` | **无** |
| 可自动改 | FixAction payload | **无** |

这是面向 Agent 升级的核心缺口。

### 1.4 Group 相关旋钮（Agent 可调、但 lint 未指引）

来自 [`group-layout-and-frame.md`](../../guides/group-layout-and-frame.md)：

| 层级 | DSL | 管什么 |
|------|-----|--------|
| L1 | diagram `group_frame` | 组间：track / gap / cross / border / 短名 strips·fit·lanes… |
| L2 | `group { layout: … }` | 组内：auto / horizontal / vertical / fan-out / fan-in / grid |
| L3 | `align` / `snap` | 节点对齐与像素量化 |
| 算法 option | `layout: architecture { group_padding: … }` | 组框内边距 |

**违规 → 旋钮的经验映射（人工知识，未编码）**：

| 违规 | 优先尝试 |
|------|----------|
| `group_overlap` | 增大 `group_frame.gap`；`track: fit`；检查拓扑/拆组 |
| `node_outside_group` / `child_group_outside_parent` | 增大 `group_padding`；改组内 `layout`；减内容高度或拆组 |
| `sibling_width_ratio` | `group_frame: strips` / `track: equal`，或 `fit` 接受不等宽；调整组内 layout |
| `edge_crosses_group_interior` | 调分层/拓扑；增大 gap；避免跨无关 group |
| `edge_on_group_border` | 多为正交走廊预期 → 忽略或 verbose 观察 |
| `unrelated_edge_trunk_merge` | 偏路由/合并语义，**少用 DSL hint** |
| `label_*_overlap` | 缩短标签；间接调布局 |

### 1.5 实现层面的问题清单

#### A. 一致性 / 正确性

1. **文档滞后**：`layout-lint.md` 规则表、预设表、strict「6 条」描述与代码（12 规则 / strict 7 条）不符。  
2. **`LintMetricsSummary` 缺字段**：`node_outside_group`、`child_group_outside_parent`、`edge_on_group_border` 只进 total，不进分项。  
3. **归因不一致**：`unrelated_edge_trunk_merge` 把边 index 放进 `entity_ids`；`edge_crossing` 只标一条边的 `edge_index`。  
4. **并行信号未统一**：Sugiyama 的 `GroupLayoutWarning`（hints）与 LayoutLint 独立，Agent 看不到同一套报告。  
5. **`sibling_width_ratio` 语义偏粗**：按 y-band 聚类，不区分 parent；与「同级 sibling」文档语义可能偏差。  
6. **`validate --layout-check` 注释写「全量」**，实现是 strict。

#### B. 架构 / 可扩展性

1. 规则用 `if cfg.is_enabled` 硬编码，无注册表；加规则要改多处（run / metrics / docs / CLI）。  
2. 违规排序键不含 severity / entity_ids，同类 message 碰撞时顺序仍稳，但归因字段相同时不够可区分。  
3. `GroupFrameReport` 文档称可在 hints 查看，实际 `apply_group_frame` 结果被丢弃，未写入 `LayoutHints`。

#### C. 性能（大图时）

| 检查 | 特征 |
|------|------|
| 节点/组重叠 | O(n²) / O(g²)，可接受 |
| 边穿节点 / 贴边框 / 穿组 | 内层重复 `sort()` |
| 边交叉、假并线 | O(E²) |
| label 重叠 | O(标签×节点) |

优化空间：外提排序、复用 `GroupInteriorMaps`、strict 跳过 warning 路径（已有）、可选采样。**不必为 lint 引入复杂空间索引**，除非 showcase 实测超时。

#### D. Agent 产品断层

1. Studio tools：`validate` / `render` / `layout_catalog`，**无 lint**。  
2. Agent prompt 几乎不谈 architecture 的 group_frame / group layout。  
3. MCP 策略文档未列 lint。  
4. 无「违规 → DSL 补丁」的结构化映射。

---

## 2. 升级目标

1. **门禁仍稳**：CI / eval 行为不退化；warning 仍可忽略（符合 `AGENTS.md` §4）。  
2. **Agent 可行动**：对每条（或每类）违规给出**可执行的 hint 调节建议**，优先 group 相关。  
3. **建议可分级**：text 给人/Agent 读；可选 `fix` payload 给自动改图（对齐 error-model，但不强制一刀切自动应用）。  
4. **成本可控**：建议层是规则表 + 轻量启发式，不引入二次布局搜索作为默认路径。

---

## 3. 方案：LintAdvice（面向 Agent 的指导层）

### 3.1 核心思路

在现有 `LayoutViolation` **之上**增加可选指导，而不是把 lint 改成「自动修布局」：

```text
LayoutViolation（事实：几何违规）
        +
LintAdvice（建议：改哪些 DSL / 为何 / 置信度）
        ↓
LintReport { violations, advices }  或  violation.advices: Vec<LintAdvice>
```

原则：

- **事实与建议分离**：检测逻辑保持纯几何；建议由独立 `advice` 模块根据 rule + 归因 + 少量上下文生成。  
- **宁可少建议，不可乱建议**：高置信度才给 `fix`；其余只给 `text` + `knobs`。  
- **优先 group hint**：Agent 最缺的是 L1/L2 旋钮指引，不是再报一遍「重叠了」。

### 3.2 建议的数据结构

```rust
/// 单条布局调节建议（对齐 DiagnosticError.suggestion 精神）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintAdvice {
    /// 关联的违规下标，或 rule + 主键
    pub violation_index: usize,
    /// 人类/Agent 可读说明
    pub text: String,
    /// 建议优先级（先试高的）
    pub priority: u8,
    /// 置信度：high | medium | low
    pub confidence: AdviceConfidence,
    /// 涉及的 DSL 旋钮（机器可读，无副作用）
    pub knobs: Vec<LayoutKnob>,
    /// 可选自动修复动作（与 FixAction 同形，便于 Agent 复用）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<FixAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LayoutKnob {
    /// L1
    GroupFrame {
        field: GroupFrameField, // Gap | Track | Cross | Preset
        suggested: String,      // "60" | "equal" | "strips" …
        rationale: String,
    },
    /// L2
    GroupLayout {
        group_id: String,
        suggested: String, // "grid" | "horizontal" | …
        rationale: String,
    },
    /// 算法 option
    LayoutOption {
        key: String,           // "group_padding"
        suggested: String,
        rationale: String,
    },
    /// 结构建议（不可自动修或需人工确认）
    Topology {
        action: String,        // "split_group" | "move_entity" | "add_constrain"
        targets: Vec<String>,
        rationale: String,
    },
    /// 标签/文案
    Label {
        edge_or_entity: String,
        action: String, // "shorten"
        rationale: String,
    },
    /// 明确建议忽略（合法噪音）
    IgnoreRule {
        rule: String,
        rationale: String,
    },
}
```

`LintReport` 扩展示例：

```rust
pub struct LintReport {
    pub violations: Vec<LayoutViolation>,
    /// 默认空；`LintConfig::with_advice(true)` 或 CLI `--advice` 时填充
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub advices: Vec<LintAdvice>,
}
```

配置开关：

```rust
impl LintConfig {
    pub fn with_advice(self, enabled: bool) -> Self;
}
```

- 默认 **关闭** advice（保持现有 JSON 兼容与 CI 噪音可控）。  
- Agent / `--advice` / Studio lint 工具默认 **开启**。

### 3.3 规则 → 建议映射表（P1 核心资产）

下表是产品知识编码目标；实现为静态表 + 少量 metric 分支即可。

| 规则 | 建议顺序（priority） | confidence | 可生成 fix？ |
|------|----------------------|------------|--------------|
| `group_overlap` | ① `group_frame.gap += Δ`（Δ 由 metric/重叠面积启发式）② 检查是否应改 `track` ③ Topology 拆组 | high→medium | gap 可试探生成；拆组否 |
| `node_outside_group` | ① `group_padding += excess + margin` ② 该组 `layout` 换更矮/更宽模式 ③ 移出实体 | high | padding 可 |
| `child_group_outside_parent` | ① 父组 padding ② 子组 layout ③ 检查嵌套是否必要 | medium | padding 可 |
| `sibling_width_ratio` | ① `group_frame: strips` 或 `track: equal` ② 或 `fit` + 说明「接受内容驱动宽度」③ 调宽组内 layout | high（architecture） | preset/track 可 |
| `edge_crosses_group_interior` | ① 增大相关 group 间距 ② 调整端点所属分层/拓扑 ③ Ignore 若实为祖先链误报（应极少） | medium | 弱 |
| `node_overlap` | ① 增大 node/layer gap（算法 option）② 拆密集 group 内 layout | medium | option 可 |
| `edge_through_node` | ① 调端口/路由相关（多非 DSL）② 挪节点所属 group / constrain | low | 通常否 |
| `edge_crossing` | ① 接受 warning ② 减少跨层边 / 加 constrain | low | 否 |
| `edge_on_group_border` | ① `IgnoreRule` + 说明走廊预期 | high | ignore 即可 |
| `label_node_overlap` / `label_label_overlap` | ① 缩短 label ② 间接调边路由 | medium | 文案弱自动 |
| `unrelated_edge_trunk_merge` | ① 说明属路由合并语义 ② 勿乱改 group_frame | high（负向） | 否 |

**Architecture 特化**：若 `diagram_type == Architecture`，sibling / group_overlap / containment 建议优先引用 L1/L2；flowchart 则弱化 `group { layout }`（分治路径不消费 L2）。

### 3.4 输出示例（Agent 友好 JSON）

```json
{
  "violations": [
    {
      "rule": "sibling_width_ratio",
      "severity": "warning",
      "message": "同级条带宽比过大：'data_ns'/512 vs 'platform_ns'/296 = 1.730",
      "metric": 1.73,
      "group_ids": ["data_ns", "platform_ns"]
    }
  ],
  "advices": [
    {
      "violation_index": 0,
      "text": "同行顶层 group 宽比 > 1.08。优先用 L1 等宽条带：写 group_frame: strips，或 group_frame { track: equal, gap: 40 }。",
      "priority": 1,
      "confidence": "high",
      "knobs": [
        {
          "kind": "group_frame",
          "field": "preset",
          "suggested": "strips",
          "rationale": "architecture 同级条带审美；showcase 中 track:equal 可将宽比压到 ≈1.0"
        }
      ],
      "fix": {
        "action": "set_group_frame_preset",
        "payload": { "preset": "strips" }
      }
    }
  ]
}
```

Agent 工作流：

```text
生成/修改 .pgm
  → validate（语法语义）
  → lint --profile default --advice --format json
  → 若有 error：按 advices 优先级改 DSL（优先 apply fix / knobs）
  → 再 lint；warning 可停（符合 AGENTS.md）
```

### 3.5 与现有模块的关系

| 模块 | 关系 |
|------|------|
| `DiagnosticError.suggestion` | **同构**：LintAdvice 复用 `FixAction` 形状；类别是 layout 而非 parse |
| `GroupLayoutWarning`（hints） | P1 后可映射进 LintReport 或单独 `hints_warnings` 字段，避免双轨 |
| `GroupFrameReport` | 建议写入 `LayoutHints`，供 advice 判断「是否已 equalized」避免重复建议 |
| friendliness `Adjustment` | 历史设想偏路由反馈环；本方案面向 **DSL Agent**，不替代内部 router 反馈 |
| `plotgram-eval` | 继续用 metrics；advice **不进** quality_score |

---

## 4. 非 Advice 的工程优化（建议同步做）

### P0 — 低成本、立刻该做

| 项 | 说明 |
|----|------|
| 同步 `layout-lint.md` | 12 规则、预设表、strict=7、validate=strict |
| 补全 `LintMetricsSummary` | 增加 containment / border 分项；`from_report` 全覆盖 |
| 统一边归因 | trunk merge 用 `edge_index` + 第二边字段或 `entity_ids` 存端点 id；crossing 标两条边 |
| 修正 CLI 注释 | `validate --layout-check` 标明 strict |
| 外提重复 sort | `check_edge_*` 内层不要反复 `group_ids.sort()` |

### P1 — Advice MVP

| 项 | 说明 |
|----|------|
| `LintAdvice` + `LayoutKnob` | 见 §3.2 |
| 静态映射表 | 至少覆盖：containment、group_overlap、sibling_width_ratio、edge_on_group_border |
| CLI `--advice` | JSON 输出带 advices；text 模式打印「建议：…」 |
| `GroupFrameReport` 入 hints | 供「已 strips 仍 overlap → 换建议」 |
| 文档 | `layout-lint.md` 增「Agent 指导」一节；链到 group-layout 指南 |

### P2 — Agent 产品面

| 项 | 说明 |
|----|------|
| Studio / WASM `lint` 工具 | 与 validate 并列；默认 advice=on |
| MCP 策略更新 | `agent-mcp-skills-strategy.md` 增加 lint |
| Agent system prompt | architecture 图强制：先 group 拓扑 → L2 layout → L1 group_frame；出错跑 lint --advice |
| 可选 auto-fix | 仅 `confidence=high` 且 `fix` 为安全 DSL 写入（gap/padding/preset）；**禁止**自动拆拓扑 |

### P3 — 可选增强（勿过早）

- 建议后「干跑」：应用 fix → 重新 layout → 对比 lint 计数（代价高，仅调试模式）。  
- 源码 Location：从 AST span 反查 group_frame / group.layout 写入点，方便 apply_patch。  
- 规则注册表宏：减少加规则时的散落修改。  
- `sibling_width_ratio` 改为「同 parent + 同 macro rank」严格 sibling。

---

## 5. 明确不做（防范围膨胀）

1. **不用 lint 驱动布局算法内部迭代**（那是 router friendliness / space budget 的事）。  
2. **不要求 Agent 消掉全部 warning**（`AGENTS.md` §4 仍然成立）。  
3. **不把 eval 分数绑到 advice 采纳率**。  
4. **不对 flowchart 强推 architecture 专用 L2 hint**（分治不读 group layout）。  
5. **不实现「全局最优 hint 搜索」**——建议是启发式查表，不是优化器。

---

## 6. 验收标准（建议）

### P0

- [ ] `layout-lint.md` 与 `LintRuleId::ALL` 一致  
- [ ] `LintMetricsSummary` 对 12 规则无静默丢弃  
- [ ] showcase 批量 `plotgram lint --profile strict` 行为与改前一致（仅文档/metrics 修复时）

### P1

- [ ] 对 `c.k8s-multi-namespace-overview` 类宽比问题，`--advice` 给出 `strips` / `track: equal`  
- [ ] 对 `node_outside_group`，建议含 `group_padding` 且 suggested ≥ excess  
- [ ] `edge_on_group_border` 建议为 IgnoreRule，不误导改 gap  
- [ ] 关闭 `--advice` 时 JSON 与现网兼容（无 advices 或空数组）

### P2

- [ ] Studio Agent 工具列表含 lint；一轮「违规 → 改 group_frame → 再 lint」可在 prompt 文档中复现  
- [ ] 高置信 fix 应用后，目标用例 error 数下降（人工抽测 5+ architecture showcase）

---

## 7. 推荐落地顺序

```text
Week 1（P0）
  文档同步 + metrics 补全 + 归因修正 + 微性能

Week 2（P1）
  LintAdvice 类型 + 4 条高价值映射（overlap / containment / sibling / border）
  CLI --advice
  GroupFrameReport → hints

Week 3（P2）
  WASM/Studio lint 工具 + prompt / MCP 文档
  安全 fix 白名单（preset / gap / padding）
```

---

## 8. 风险与对策

| 风险 | 对策 |
|------|------|
| 错误建议导致 Agent 改坏图 | confidence 分级；fix 白名单；默认不自动 apply |
| advice 让 CI 变吵 | advice 默认关；strict CI 不开 `--advice` |
| 与算法开发干扰 | AGENTS.md 仍允许保留 warning；advice 面向写 DSL 的 Agent |
| 维护成本（规则×旋钮矩阵） | 先做 4 条高频规则；矩阵放 `lint/advice/map.rs` 单文件 |

---

## 9. 附录：Agent Prompt 片段（草案）

```text
布局自检（architecture）：
1. plotgram validate <file>
2. plotgram lint <file> --advice --format json
3. 若存在 error：按 advices[].priority 修改 DSL
   - group 重叠/越界 → 优先 group_padding、group_frame.gap、组内 layout
   - 同级宽比过大 → group_frame: strips 或 track: equal
   - edge_on_group_border → 可忽略（正交走廊）
4. warning 不必清零；error 清零后再 render
5. 不要手写节点坐标；用 group { layout } 与 group_frame 调节
```

参考指南：[`group-layout-and-frame.md`](../../guides/group-layout-and-frame.md)。

---

## 10. 总结

| 层面 | 现状 | 建议 |
|------|------|------|
| 检测 | 12 规则，几何扎实 | 补文档/metrics/归因；微优化 |
| 指导 | 无 | 新增 LintAdvice → LayoutKnob，优先 group L1/L2 |
| 产品 | CLI 有、Agent 无 | Studio/MCP lint + prompt 闭环 |
| 哲学 | 报错 | 报错 + **可执行的 hint 调节意见**（对齐 error-model） |

LayoutLint 升级的关键不是「再多几条规则」，而是把已经存在于文档里的 **group hint 调节知识**，编码成 Agent 能消费的结构化建议。
