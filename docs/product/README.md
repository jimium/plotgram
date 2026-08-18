# Tautcore 产品与商业化备忘

> 状态：proposed（讨论总结，待验证）  
> 日期：2026-08-17  
> 定位：从「重建 tautcore」到「如何立足市场」的战略共识。**非 ADR、非引擎设计档**；市场判断是推断，需用真实潜在客户验证。引擎设计与写权纪律仍以 `docs/design/` 与 `AGENTS.md` 为准。

---

## 0. 一句话

**产品是引擎，agent / skill / Studio 是分发，团队资产是锁定。**

- 卖的：**引擎**（graph → 确定性的高质量布局），以嵌入式 SDK / 托管引擎 API 卖给有图数据的产品团队。
- 免费送：CLI / WASM / skill / MCP / Studio，它们是**获客渠道**，不是收入。
- 收费锁：团队资产治理（私有 theme/profile/archetype）+ 专有资产（主题包 / archetype 目录 / benchmark）。

---

## 1. 现状（我们有什么）

**布局内核**（`crates/tautcore-layout/src/layout/`）：`hierarchical`（主核，Sugiyama）、`tree`、`sequence`、`circular`。

**独立边路由**（`tautcore-router/`）：`orthogonal`（reduced OVG + A*）、`straight`、`polyline`、`octilinear`、`curved`；`bus` 后置。

**引擎之外的差异化**（真正的护城河）：

- 固定语法 + 结构化诊断（非 Mermaid 的「隐式规则 + 静默失败」）
- AST 一等公民：JSON 导出、语义 diff / patch
- **ADR-007「布局事实描述层」**：引擎写、LLM 读的几何只读通道（`--explain`）
- 单写者纪律 + 确定性 + 图种只进 profile（ADR-001）

**产品面**（`apps/`）：`studio`（LLM 驱动绘图工作台）、`agent-demo`、`inspector`、`playground`、`editors`、`website`、`showcase`。

> 已知缺口：README 仍写「force-directed」，但重建内核只有 4 个，**Organic（力导向）是当前缺口**，却是 AI 生成图最高频的一类。

---

## 2. 市场定位

**不在红海跟 Mermaid 拼人类手写图**（饱和、网络效应强、模型对 Mermaid 训练量碾压级）。

**赢在 AI 生成图**：LLM 生成图的三痛点恰好是 Tautcore 的强项——语法不可靠、布局丑、无法「改一版再校验」。

**定位**：AI agent 的**确定性渲染/布局后端**，不是「又一个画图 DSL」。

---

## 3. 产品形态：卖引擎，不卖 agent/UI

关键区分（讨论中反复纠正出来的核心结论）：

| 层 | 是什么 | 角色 | 收费 |
|----|--------|------|------|
| **引擎（engine）** | graph → 确定性高质量布局 | **产品本体 / 收入** | ✅ 嵌入 SDK / 托管引擎 API |
| **Agent**（skill / Studio / NL） | 让引擎好够到的入口 | **分发 / 获客** | 免费或极薄 |
| **团队资产治理** | theme/profile/archetype 托管 | **锁定** | ✅ 治理订阅 |

**明确不做**：

- 不做**消费者产品**（Studio 面向非程序员）：非程序员看不到护城河（确定性 / 布局事实 / AST diff），你会被拖去跟 Napkin / Eraser / Lucidchart AI 拼 UX + 分发，引擎优势退化成「及格线」。
- 不做**团队协作**（多人在线同画 / 权限 / 评论 / 实时同步）：最贵、最卷、拼网络效应，是「另一家公司」。仅当出现明确的企业流失信号再议。
- 不追 yFiles 全栈 parity、不以 Mermaid 语法兼容为主目标、不铺一堆半成品内核。

**Studio 的定位**：`apps/studio/src/agent/` 的 `AgentLoop` + `tools` 就是「托管绘图 Agent」的**参考实现**。Studio 不是独立产品，是那个 API 的人形前端 + 免费 demo。

---

## 4. 分发（免费层）

免费层换：**采纳 + 生态占位 + 使用反馈 + 内部 champion**。没有盈利点，也不该有。

### 4.1 Agent skill（最强分发渠道）

- **Skill 是知识层，不是运行时**：教 agent「何时用、怎么写 `.taut`、怎么调引擎画与迭代」；渲染靠运行时。
- **运行时默认走 CLI**（本地、免费、隐私、零门槛），**托管 API 是可选付费升级**，经 `TAUTCORE_API_URL` + key 切换 transport，skill 正文不写死。
- **`--explain` 必须免费**：它是让 agent 闭环「画完 → 读交叉数/绕向 → 自己迭代」的核心，也是 Mermaid 给不了 agent 的钩子。
- **平台覆盖**：Zed / Claude Code / Claude Desktop = `SKILL.md`（`name` + `description` frontmatter + 指令体 + 支持文件）；Cursor = MCP + `.cursor/rules`（不原生用 SKILL.md）。

### 4.2 形态清单

| 形态 | 角色 | 收费 |
|------|------|------|
| crate + WASM + CLI | 本体 | 免费 |
| skill（SKILL.md） | 知识层 / 分发 | 免费 |
| MCP（本地） | 稳的 tool-calling | 免费 |
| Studio / agent-demo | 参考实现 / demo | 免费 |

---

## 5. 商业化（付费层）

付费墙不在「渲染」上（渲染被 WASM/开源商品化），在**免费本地路径做不到的事**上。

| 墙 | 是什么 | 强度 | 为什么 |
|----|--------|------|--------|
| **嵌入引擎 SDK / 托管引擎 API** | graph → 图，SLA、确定性缓存、不养 Rust 工具链、support | **强** | yFiles 靠这个卖了几十年；已有现成预算 |
| **团队资产治理** | 私有 theme/profile/archetype + RBAC/SSO/审计 + 版本 | **强** | 锁定 + 治理，换平台成本高 |
| **专有资产** | 主题包 / archetype 目录 / 布局质量 benchmark / eval 集 | 中 | 资产独立于引擎代码，可单独收费 |
| 纯渲染端点 / 确定性规模 + SLA | 「render .taut → SVG」 | **弱** | 可自托管，卖的是信任不是功能 |

**目标客户**（谁会用、什么场景）：

- observability / 云控制台 / K8s 平台：把基础设施拓扑实时画成图。
- IaC / 依赖可视化 / 供应链安全：渲染资源依赖、攻击路径。
- BI 数据血缘 / ER：把 schema / 血缘画成图。
- 工作流 / 低代码构建器：把用户搭的流程实时可视化。
- 文档 / RAG 平台：回答里附带架构/时序图。

这些客户**已有 graph 数据、今天已在付钱或养人**（yFiles 贵、dagre 旧、自研养团队）。楔子：**yFiles 的质量 + 开源/API 的价格 + 确定性 + agent 友好**。

---

## 6. 开源与 license

**原则**：MIT 商品化的是**代码**，不是**生意**。卖的是代码之外的东西——运维、治理、支持、专有资产。

**切分**（crate 边界天然支持）：

| 开源（MIT，铺量） | 可闭源 / 可收费 |
|------------------|----------------|
| `engine` / `layout` / `router` / `parse` / `render` | 托管引擎服务（重建 `tautcore-server`） |
| CLI / WASM / skill | 团队资产治理（私有 theme/profile/archetype + RBAC/SSO/审计） |
| 基础主题 | 高级主题 / archetype 资产包 |
| `--explain` 布局事实 | benchmark / eval 数据集（ADR-007「优先做 eval 集」） |

**License 选型**：

- 对**变现**两者等价（都 permissive、非 copyleft）。
- 对**嵌入型 B2B2D 采纳**，Apache 2.0 略友好——**明示专利授权**消除大公司法务的专利顾虑。
- 对**早期铺量**，MIT 更省心、贡献门槛更低。

**决定**：保持 MIT 到「商业团队真的在嵌入」再评估换 Apache 2.0。理由：当前采纳瓶颈是「有没有人用」不是「专利条款」；换 license 有历史贡献者授权问题，现在换成本高收益小。

---

## 7. 相关专题

| 文档 | 内容 |
|------|------|
| [layout-facts-corpus-and-benchmark.md](layout-facts-corpus-and-benchmark.md) | PNG+PGM+explain 三元组、VLM 语料/benchmark 价值、对外商业化、相对 Mermaid/Graphviz 差异化 |

---

## 8. 待验证 / 开放问题

1. **需求验证**：从 observability / IaC / BI 血缘 / 工作流 中选 3 个场景，各配「客户是谁、今天怎么解决、你的差异化、怎么触达」，直接拿去对潜在客户。
2. **Organic 内核**：AI 生成图最高频场景（网络 / 知识图谱），当前空白，是否优先立项。
3. **Studio 抽离成本**：`apps/studio/src/agent/` 的 `AgentLoop` / `tools` 抽成服务端 API 的距离，决定墙 1 落地时长。
4. **托管服务重建**：`tautcore-server` 当前缺 crate（README 有雏形、代码无），落地需重建。
5. **付费墙顺序**：先墙 2（团队资产治理，复用已有规范、成本最低、与 skill 漏斗咬合最紧）？还是先墙 1（收入天花板高但投入大）？
6. **Benchmark 商业化**：见 [layout-facts-corpus-and-benchmark.md](layout-facts-corpus-and-benchmark.md) §6 开放问题（DLUB 题型、垂直选型、import 优先级）。
