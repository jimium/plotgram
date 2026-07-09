# 删除 Layout Intent，改用 DSL 隐形约束边

> 日期：2026-07-09  
> 状态：已决定，待实施  
> 相关文档：  
> - [layout-routing-pipeline-full-analysis.md](./layout-routing-pipeline-full-analysis.md)（§14 intent 现状）  
> - [layout-routing-optimization-proposal-2026-07.md](./layout-routing-optimization-proposal-2026-07.md)（布局/路由主优化，与本决策正交）  
> - 既有 intent 设计稿（将随删除一并废弃）：`docs/architecture/intent/`、`docs/guides/layout-intent.md`

---

## 0. 一句话结论

**删除整套 Layout Intent（overlay / Pin / Align / RefinementReport）**；  
需要「影响分层、但不画边」时，改用 DSL 声明：

```plotgram
constrain A -> B
```

约束跟图源码走，可复现；不再在渲染期另传 overlay。

---

## 1. 背景与判断

### 1.1 Intent 解决的问题

Intent 设计目标是：不修改 DSL、不增加 `relations` 的前提下，在**渲染期**注入局部布局约束（`Above`/`Below`、`Pin`、`Align*`），并返回满足度报告。

### 1.2 为何判定为过度设计

| 维度 | 观察 |
|------|------|
| 与主矛盾无关 | 架构图条带审美、通道预算、流程图 sink/spine 等质量问题不依赖 intent |
| 能力薄、触点广 | 拓扑仅 Above/Below，几何仅 Pin/Align，却贯穿 pipeline / grid_snap / group_frame / 多 strategy / wasm / server / playground |
| 关键场景半残 | architecture 跨组意图边跳过；flowchart 有 group 时忽略 overlay |
| 使用面弱 | showcase / eval 主路径基本不带 `layout_intents` |
| 信息放错层 | 约束是图语义的一部分，却放在渲染参数里，难持久、难复现 |

引擎里真正有用的只有一步：往分层图注入 **`reversible=false` 的有向边**。其余（意图枚举、满足度状态机、PinSet、geometric 精修）是产品包装，当前阶段不值得保留。

### 1.3 与「拓扑约束边 API」的取舍

曾考虑把 intent 收敛成渲染期「约束边 API」。最终决定：

- **不保留**渲染期 overlay / 约束边 API  
- **改为** DSL 隐形边（`constrain`）  
- 约束持久化在源码中，showcase / 调试 / LLM 生成均可直接使用  

---

## 2. 决策

### 2.1 删除 Layout Intent

删除（或等价清空后移除）以下能力与触点：

| 范围 | 内容 |
|------|------|
| Core | `layout/intent/`（topology / geometric / PinSet / RefinementReport 等） |
| Pipeline | `validate_topology_intents`、`evaluate_topology_satisfaction`、`apply_geometric_refinement`、`check_alignment_after_refine`、空 `PinSet` 贯穿 |
| Strategy | `compute_with_overlay` 及各算法对 `ValidTopologyIntent` 的注入分支 → 恢复为单一 `compute`（约束改由 DSL → AST 注入） |
| API | `RenderRequest.layout_overlay`、`layout_intents`（wasm / server） |
| Playground | Intent 面板、`intentOptions`、refinement_report UI |
| 文档 | `docs/architecture/intent/`、`docs/guides/layout-intent.md` 等标记废弃或删除 |

项目未对外发布，**不做向后兼容**（符合 `AGENTS.md` §1）。

### 2.2 新增 DSL 隐形约束边

**语法（唯一主语法）：**

```
<constraint_declaration> ::= "constrain" <identifier> "->" <identifier>
```

示例：

```plotgram
diagram flowchart {
    entity[start] start "开始"
    entity[process] a "处理"
    entity[process] side "旁路"
    entity[end] end "结束"

    start -> a
    a -> side
    side -> a
    a -> end

    // 无业务边，但强制 end 在下游（沉底）
    constrain side -> end
}
```

**不采用：**

- 新箭头字符（`~>` / `..>` / `=>` 等）——与「仅保留 `->` / `-->` / `<->`」冲突  
- `a -> b { invisible: true }` / `layout: constraint`——易与样式/隐藏业务边混淆  
- `rank A before B` 等第二套语法——首期只保留 `constrain`  
- 渲染期 `layout_intents` overlay——与本决策冲突  
- 隐形边带 label / 端点标签——无视觉载体，禁止  

---

## 3. 语义规范

| 项 | 约定 |
|----|------|
| 方向 | `constrain A -> B` ⇒ A 上游、B 下游（`rank(A) < rank(B)`） |
| 箭头 | 仅允许 `->`；`-->` / `<->` 在 `constrain` 上为语法错误 |
| 渲染 | 不画线、无箭头、无标签 |
| 路由 | 不进入正交路由 / bundling / 可见边 lint 的边集合 |
| FAS | 不可反转（等同现 `EdgeMeta { reversible: false }`） |
| 与真实边 | 可并存；同向真实边 + constrain 冗余但合法 |
| 自环 | prepare / 校验 **报错** |
| 成环 | 与真实边 + 其他 constrain 成环时 **报错**（禁止静默跳过） |
| 跨组 | **允许**；若某算法路径暂未实现，必须 **显式报错**，禁止静默忽略 |
| 标签 | 不允许 `constrain a -> b "xxx"` |

调试期若需可视化约束，用独立调试开关（例如将来的 debug 层画辅助线），**不**把「画出约束」做成默认 DSL 语义。

---

## 4. AST 与管线落点

### 4.1 数据结构（建议）

**不要**把约束塞进普通 `relations` 再靠 flag 过滤（易漏过滤、破坏 `relations[i] ↔ edges[i]`）。

```text
Diagram {
  entities, groups, ...
  relations: Vec<Relation>,        // 仅可渲染边
  constraints: Vec<Constraint>,    // { from, to }，仅布局用
}
```

`Constraint` 最小字段：`from`、`to`、`span`（错误定位）。

### 4.2 消费点

1. **Parse**：识别 `constrain A -> B`，写入 `constraints`  
2. **Validate / Prepare**：节点存在性、自环、成环  
3. **Layout 建图**：与真实边一并建图；约束边 `reversible=false`  
4. **Route / Render / Export scene**：只遍历 `relations`，忽略 `constraints`  

删除 intent 后，现有 `inject_intent_edges` / `build_graph_with_overlay` 应改为消费 `diagram.constraints`（或等价内部结构），不再接收 overlay 参数。

---

## 5. 实施顺序（建议）

1. **先删 intent**  
   主路径恢复为无 overlay；确认 showcase / 默认渲染无行为回归。  
2. **再加 `constrain`**  
   语法 → AST → 校验 → 布局注入 → 文档与 1～2 个 showcase 样例。  
3. **不并行两套**  
   实施期间不要保留「半套 intent + 半套 constrain」。

与 [layout-routing-optimization-proposal-2026-07.md](./layout-routing-optimization-proposal-2026-07.md) 的 RankBand / 通道预算等工作**正交**，可并行，但勿把 constrain 做成那些优化的依赖。

---

## 6. 明确不做

1. 不保留 `Pin` / `AlignVertical` / `AlignHorizontal`（坐标后处理另议，不进本 DSL）  
2. 不保留 `RefinementReport` / `IntentStatus`  
3. 不保留渲染期 `layout_overlay` / `layout_intents`  
4. 不做 SameRank / Near / 通用约束求解  
5. 不为「消掉 intent 相关 warning」堆兼容层  

---

## 7. 验收标准

| 项 | 标准 |
|----|------|
| Intent 清除 | 无 overlay API；pipeline 无 PinSet / RefinementReport 主路径分支 |
| 语法 | `constrain A -> B` 可解析；带 label / 错误箭头 被拒绝 |
| 布局 | 约束影响 rank；FAS 不反转约束边 |
| 视觉 | SVG/场景中无约束边几何 |
| 索引 | `relations.len()` 与可见 `edges.len()` 仍一一对应 |
| 错误 | 自环、成环、未知节点 → 明确诊断（含 span） |
| 回归 | 无 constrain 的既有 showcase 与删 intent 前一致（容差内） |

---

## 8. 总结

| 旧 | 新 |
|----|----|
| 渲染期 Intent overlay | 删除 |
| `Above` / `Below` / Pin / Align + Report | 删除 |
| 「不改 DSL 的临时微调」 | 当前不做；需要时改 DSL |
| 隐形拓扑约束 | `constrain A -> B`（DSL 一等声明） |

**原则：** 约束是图的一部分，不是渲染旁路；引擎只吃不可反转、不渲染的有向边；语法保持显式、单一、与现有三箭头体系兼容。
