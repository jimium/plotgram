# 开发任务规划

本文档定义 Drawify Studio 的开发任务分解、优先级、验收标准与责任人分配。

## 1. 阶段总览

| 阶段 | 目标 | 核心交付 | 优先级 |
|------|------|---------|--------|
| P0 | 最小可用闭环 | 单轮对话生成图表 | P0(最高) |
| P1 | 增量编辑能力 | 多轮对话迭代 + Diff 可视化 | P0 |
| P2 | 智能增强 | 错误自修复 + 上下文感知 | P1 |
| P3 | 高级能力 | 模板库 + 风格切换 + 多文档 | P2 |

## 2. P0 阶段:最小可用闭环

**目标**:用户输入自然语言,Agent 生成图表并渲染。

### 任务 P0-1:WASM 桥接层验证

| 项 | 内容 |
|----|------|
| 任务 | 验证现有 drawify-wasm 的 render/validate/parse/layout_catalog 接口可用 |
| 输入 | drawify-wasm crate 已构建的产物 |
| 输出 | `src/lib/wasm.ts` 中 loadWasm/renderSource/validateSource 可正常调用 |
| 验收标准 | `npm run test` 通过 wasm.test.ts;浏览器中能调用 render 生成 SVG |
| 优先级 | P0 |
| 责任人 | 前端开发 |
| 依赖 | drawify-wasm 已构建 |

### 任务 P0-2:LLM 客户端实现

| 项 | 内容 |
|----|------|
| 任务 | 实现 OpenAI 兼容的 LLM 客户端,支持 tool-calling |
| 输入 | LLMConfig 配置 |
| 输出 | `src/lib/llm.ts` 中 createLLMClient 可用 |
| 验收标准 | llm.test.ts 通过;能调用 OpenAI API 获取带 tool_calls 的响应 |
| 优先级 | P0 |
| 责任人 | 前端开发 |
| 依赖 | 无 |

### 任务 P0-3:Agent Loop 核心实现

| 项 | 内容 |
|----|------|
| 任务 | 实现 Agent 循环引擎,支持多轮 Tool 调用 |
| 输入 | LLMClient + ToolExecutors |
| 输出 | `src/agent/AgentLoop.ts` 中 runAgentLoop 可用 |
| 验收标准 | AgentLoop.test.ts 通过;mock LLM 下能完成"思考→工具→回复"循环 |
| 优先级 | P0 |
| 责任人 | 前端开发 |
| 依赖 | P0-1、P0-2 |

### 任务 P0-4:Tool 执行器实现

| 项 | 内容 |
|----|------|
| 任务 | 实现 render/validate/parse/layout_catalog 四个 Tool 执行器 |
| 输入 | WASM 模块 |
| 输出 | `src/agent/tools.ts` 中 createToolExecutors 可用 |
| 验收标准 | Tool 能正确调用 WASM 并返回结构化结果 |
| 优先级 | P0 |
| 责任人 | 前端开发 |
| 依赖 | P0-1 |

### 任务 P0-5:useAgent Hook 实现

| 项 | 内容 |
|----|------|
| 任务 | 实现 useAgent Hook,管理对话状态与 Agent 调用 |
| 输入 | wasm、ready |
| 输出 | `src/hooks/useAgent.ts` 可用 |
| 验收标准 | 能发送消息、接收 Agent 回复、更新 currentSvg |
| 优先级 | P0 |
| 责任人 | 前端开发 |
| 依赖 | P0-3、P0-4 |

### 任务 P0-6:对话区 UI 实现

| 项 | 内容 |
|----|------|
| 任务 | 实现 ChatPanel、ChatMessage 组件 |
| 输入 | useAgent 返回的 messages/isRunning |
| 输出 | 对话区可输入、显示消息、显示 Agent 回复 |
| 验收标准 | 用户输入后能看到 Agent 回复;Agent 执行中显示状态 |
| 优先级 | P0 |
| 责任人 | 前端开发 |
| 依赖 | P0-5 |

### 任务 P0-7:预览区 UI 实现

| 项 | 内容 |
|----|------|
| 任务 | 实现 PreviewCanvas 组件,显示 SVG,支持缩放/平移 |
| 输入 | useAgent 返回的 currentSvg |
| 输出 | 预览区显示渲染结果 |
| 验收标准 | SVG 正确显示;滚轮缩放可用;适应窗口可用 |
| 优先级 | P0 |
| 责任人 | 前端开发 |
| 依赖 | P0-5 |

### 任务 P0-8:端到端联调

| 项 | 内容 |
|----|------|
| 任务 | 联调 Agent Loop + LLM + WASM,完成单轮对话闭环 |
| 输入 | 所有 P0 任务完成 |
| 输出 | 用户输入"画一个微服务架构图"→ Agent 生成 DSL → 渲染 SVG |
| 验收标准 | 端到端流程跑通;无控制台错误;渲染结果合理 |
| 优先级 | P0 |
| 责任人 | 前端开发 + 测试 |
| 依赖 | P0-1 ~ P0-7 |

**P0 验收标准(阶段级)**:
- 用户能通过自然语言生成至少 3 种图表类型(flowchart/architecture/sequence)
- Agent 单轮对话在 30 秒内完成
- 渲染结果与 Playground 一致

## 3. P1 阶段:增量编辑能力

**目标**:支持多轮对话增量修改图表,展示变更差异。

### 任务 P1-1:drawify-wasm 新增 diff_sources 绑定

| 项 | 内容 |
|----|------|
| 任务 | 在 drawify-wasm crate 新增 diff_sources 导出 |
| 输入 | drawify-core 的 diff::diff 函数 |
| 输出 | WASM 中可调用 diff_sources(old, new) → DiffResult JSON |
| 验收标准 | Rust 单元测试通过;JS 端能解析返回的 DiffResult |
| 优先级 | P0 |
| 责任人 | Rust 开发 |
| 依赖 | 无 |

### 任务 P1-2:drawify-core 新增 ast_to_source 能力

| 项 | 内容 |
|----|------|
| 任务 | 在 drawify-core 新增 Diagram → DSL 文本的序列化能力 |
| 输入 | Diagram AST 结构 |
| 输出 | `ast_to_source(diagram) → String` 函数 |
| 验收标准 | 能将 AST 反序列化为合法 DSL;round-trip(parse→serialize→parse)一致 |
| 优先级 | P0 |
| 责任人 | Rust 开发 |
| 依赖 | 无 |

### 任务 P1-3:drawify-wasm 新增 apply_patch 绑定

| 项 | 内容 |
|----|------|
| 任务 | 在 drawify-wasm crate 新增 apply_patch 导出 |
| 输入 | drawify-core 的 diff::apply_patch + ast_to_source |
| 输出 | WASM 中可调用 apply_patch(source, patch_json) → 新 DSL |
| 验收标准 | Rust 单元测试通过;JS 端能应用 patch 并获取新 DSL |
| 优先级 | P0 |
| 责任人 | Rust 开发 |
| 依赖 | P1-2 |

### 任务 P1-4:Studio WASM 桥接扩展

| 项 | 内容 |
|----|------|
| 任务 | 在 `src/lib/wasm.ts` 新增 diffSources/applyPatch 函数 |
| 输入 | P1-1、P1-3 的 WASM 绑定 |
| 输出 | TS 端可调用 diffSources/applyPatch |
| 验收标准 | wasm.test.ts 中 diff/applyPatch 测试通过 |
| 优先级 | P0 |
| 责任人 | 前端开发 |
| 依赖 | P1-1、P1-3 |

### 任务 P1-5:Tool 执行器扩展

| 项 | 内容 |
|----|------|
| 任务 | 在 tools.ts 新增 diff/apply_patch Tool 执行器与 Schema |
| 输入 | P1-4 的桥接函数 |
| 输出 | Agent 可调用 diff/apply_patch Tool |
| 验收标准 | Agent 能通过 apply_patch 增量修改 DSL;能通过 diff 获取变更 |
| 优先级 | P0 |
| 责任人 | 前端开发 |
| 依赖 | P1-4 |

### 任务 P1-6:DiffSummary 组件实现

| 项 | 内容 |
|----|------|
| 任务 | 实现 DiffSummary 组件,可视化展示 Change 列表 |
| 输入 | DiffResult |
| 输出 | 变更摘要 UI,区分新增/删除/修改 |
| 验收标准 | 能正确显示 +N/-N/~N 统计;每条变更显示路径与描述 |
| 优先级 | P0 |
| 责任人 | 前端开发 |
| 依赖 | 无(可并行) |

### 任务 P1-7:变更接受/拒绝机制

| 项 | 内容 |
|----|------|
| 任务 | 在 useAgent 中实现 pendingSource/acceptChanges/rejectChanges |
| 输入 | Agent 的 apply_patch 结果 |
| 输出 | 用户可接受或拒绝 Agent 的变更 |
| 验收标准 | 接受后 currentSource 更新;拒绝后回滚到原版本 |
| 优先级 | P0 |
| 责任人 | 前端开发 |
| 依赖 | P1-5 |

### 任务 P1-8:多轮对话联调

| 项 | 内容 |
|----|------|
| 任务 | 联调多轮对话:生成→增量修改→Diff 展示→接受/拒绝 |
| 输入 | 所有 P1 任务完成 |
| 输出 | 完整的增量编辑闭环 |
| 验收标准 | "加个缓存""改个标签"等指令能正确执行并展示变更 |
| 优先级 | P0 |
| 责任人 | 前端开发 + 测试 |
| 依赖 | P1-1 ~ P1-7 |

**P1 验收标准(阶段级)**:
- Agent 能通过 apply_patch 增量修改至少 5 种场景(增删实体、改属性、加关系)
- Diff 摘要准确反映变更
- 变更接受/拒绝机制工作正常

## 4. P2 阶段:智能增强

**目标**:Agent 能自修复错误,具备上下文感知能力。

### 任务 P2-1:错误自修复循环优化

| 项 | 内容 |
|----|------|
| 任务 | 优化 Prompt,指导 Agent 根据 drawify 错误码自动修复 |
| 输入 | drawify-core 的结构化诊断(错误码+行号+修复建议) |
| 输出 | Agent 遇到 E003/E004 等错误时自动修正 DSL |
| 验收标准 | 故意生成错误 DSL,Agent 能在 3 次迭代内修复 |
| 优先级 | P1 |
| 责任人 | 前端开发 |
| 依赖 | P1 完成 |

### 任务 P2-2:parse Tool 上下文感知

| 项 | 内容 |
|----|------|
| 任务 | Agent 在修改前先调用 parse 理解当前图表结构 |
| 输入 | parse Tool |
| 输出 | Agent 修改时能引用正确的实体 ID |
| 验收标准 | "给第二个服务加缓存"等模糊指令能正确定位实体 |
| 优先级 | P1 |
| 责任人 | 前端开发 |
| 依赖 | P1 完成 |

### 任务 P2-3:对话历史压缩优化

| 项 | 内容 |
|----|------|
| 任务 | 优化 compactHistory,保留关键上下文(当前 DSL、最近变更) |
| 输入 | 对话历史 |
| 输出 | 压缩后历史不丢失关键信息 |
| 验收标准 | 长对话(50+条)后 Agent 仍能正确理解上下文 |
| 优先级 | P1 |
| 责任人 | 前端开发 |
| 依赖 | P1 完成 |

### 任务 P2-4:layout_catalog Tool 应用

| 项 | 内容 |
|----|------|
| 任务 | Agent 能根据需求选择合适布局算法 |
| 输入 | layout_catalog Tool |
| 输出 | "用圆形布局""改为左到右"等指令能执行 |
| 验收标准 | Agent 能查询并应用非默认布局算法 |
| 优先级 | P1 |
| 责任人 | 前端开发 |
| 依赖 | P1 完成 |

### 任务 P2-5:DSL 查看器增强

| 项 | 内容 |
|----|------|
| 任务 | DslViewer 增加语法高亮、行号 |
| 输入 | 当前 DSL |
| 输出 | 更易读的 DSL 展示 |
| 验收标准 | DSL 关键字高亮;行号显示 |
| 优先级 | P2 |
| 责任人 | 前端开发 |
| 依赖 | 无 |

**P2 验收标准(阶段级)**:
- Agent 错误自修复成功率 > 80%
- 长对话上下文不丢失
- 支持布局算法切换

## 5. P3 阶段:高级能力

**目标**:模板库、风格切换、多文档支持。

### 任务 P3-1:预设模板库

| 项 | 内容 |
|----|------|
| 任务 | 提供常见图表模板,Agent 可基于模板生成 |
| 输入 | 模板 DSL 集合 |
| 输出 | "用微服务模板画一个"等指令可执行 |
| 验收标准 | 至少 10 个模板可用 |
| 优先级 | P2 |
| 责任人 | 前端开发 |
| 依赖 | P2 完成 |

### 任务 P3-2:风格切换

| 项 | 内容 |
|----|------|
| 任务 | Agent 能切换主题与图形风格 |
| 输入 | render 的 options 参数 |
| 输出 | "用深色主题""改为手绘风格"等指令可执行 |
| 验收标准 | 主题与风格切换生效 |
| 优先级 | P2 |
| 责任人 | 前端开发 |
| 依赖 | P2 完成 |

### 任务 P3-3:多文档支持

| 项 | 内容 |
|----|------|
| 任务 | 支持多个图表文档,可切换 |
| 输入 | IndexedDB |
| 输出 | 文档列表、新建、切换、删除 |
| 验收标准 | 能管理多个图表,各自独立 |
| 优先级 | P2 |
| 责任人 | 前端开发 |
| 依赖 | P2 完成 |

### 任务 P3-4:Agent 记忆

| 项 | 内容 |
|----|------|
| 任务 | Agent 记住用户偏好(常用图表类型、风格) |
| 输入 | 历史对话 |
| 输出 | 后续对话自动应用偏好 |
| 验收标准 | 偏好持久化,跨会话生效 |
| 优先级 | P2 |
| 责任人 | 前端开发 |
| 依赖 | P3-3 |

### 任务 P3-5:导出增强

| 项 | 内容 |
|----|------|
| 任务 | 支持 PNG/WebP/ASCII/JSON 多格式导出 |
| 输入 | ExportActions 组件 |
| 输出 | 多格式导出可用 |
| 验收标准 | 各格式导出文件正确 |
| 优先级 | P2 |
| 责任人 | 前端开发 |
| 依赖 | 无 |

**P3 验收标准(阶段级)**:
- 模板库可用
- 风格切换生效
- 多文档管理正常

## 6. 责任人分配

| 角色 | 职责 |
|------|------|
| Rust 开发 | drawify-core 新增能力、drawify-wasm 绑定扩展 |
| 前端开发 | Studio 前端全部实现(Agent、UI、Hooks) |
| 测试 | 单元测试、端到端测试、验收测试 |
| 产品 | 需求确认、验收标准制定 |

## 7. 验收标准总览

### 7.1 功能验收

| 验收项 | P0 | P1 | P2 | P3 |
|--------|----|----|----|----|
| 自然语言生成图表 | 必须 | - | - | - |
| 多轮对话迭代 | - | 必须 | - | - |
| 变更 Diff 可视化 | - | 必须 | - | - |
| 错误自修复 | - | - | 必须 | - |
| 模板库 | - | - | - | 必须 |
| 多文档 | - | - | - | 必须 |

### 7.2 质量验收

| 指标 | 目标 |
|------|------|
| 单元测试覆盖率 | agent/lib 模块 ≥ 90% |
| 端到端响应时间 | 单轮对话 ≤ 30s |
| Agent 错误自修复率 | ≥ 80% |
| 控制台错误数 | 0 |

### 7.3 文档验收

| 文档 | 状态 |
|------|------|
| README.md | 已完成 |
| docs/architecture.md | 已完成 |
| docs/api.md | 已完成 |
| docs/development.md | 已完成 |
| docs/deployment.md | 已完成 |
| docs/tasks.md | 已完成(本文档) |

## 8. 风险与对策

| 风险 | 影响 | 对策 |
|------|------|------|
| ast_to_source 实现复杂 | P1 阻塞 | 优先实现最小可用版本,后续优化格式 |
| LLM tool-calling 不稳定 | Agent 行为不可控 | 限制最大迭代次数;优化 Prompt |
| WASM 体积过大 | 加载慢 | release 构建优化;wasm-opt 压缩 |
| 跨域问题 | LLM 请求失败 | 提供 Nginx 代理方案;推荐 Ollama 本地部署 |
| 长对话上下文丢失 | Agent 理解偏差 | 压缩历史;注入当前 DSL 状态 |

## 9. 里程碑

| 里程碑 | 内容 | 阶段 |
|--------|------|------|
| M1 | P0 完成,单轮对话可用 | P0 |
| M2 | P1 完成,增量编辑可用 | P1 |
| M3 | P2 完成,智能增强可用 | P2 |
| M4 | P3 完成,高级能力可用 | P3 |
| M5 | 正式发布 | 全部完成 |

每个里程碑需通过对应阶段的验收标准方可进入下一阶段。
