# 架构设计

## 1. 总体架构

Drawify Studio 是 LLM 驱动的 Agent 绘图工作台,核心是 Agent Loop 循环引擎。Agent 通过 Tool-Calling 操控 drawify-wasm,用户用自然语言对话驱动图表生成与迭代。

```
┌─────────────────────────────────────────────────────────────┐
│                    Drawify Studio (React)                    │
├──────────────────────────────┬──────────────────────────────┤
│     预览区(SVG 渲染)          │     对话区(自然语言交互)      │
├──────────────────────────────┴──────────────────────────────┤
│                      useAgent Hook                           │
├──────────────────────────────────────────────────────────────┤
│                      Agent Loop 引擎                         │
│   THINKING → TOOL_EXEC → EVALUATE → RESPOND                  │
├──────────────────────────────────────────────────────────────┤
│                   Tool 执行器(6 个)                          │
│   render | validate | parse | diff | apply_patch | catalog   │
├──────────────────────────────────────────────────────────────┤
│                   LLM 客户端(OpenAI 兼容)                    │
├──────────────────────────────────────────────────────────────┤
│                   drawify-wasm 桥接层                        │
├──────────────────────────────────────────────────────────────┤
│                   drawify-core (Rust)                        │
│   parse → prepare → validate → layout → scene → encode       │
└──────────────────────────────────────────────────────────────┘
```

## 2. Agent Loop 循环引擎

Agent 的核心是循环执行,直到完成任务或达到最大迭代次数。

### 2.1 状态流转

```
IDLE(等待用户输入)
  ↓ 用户消息
THINKING(LLM 推理)
  ↓ 选择 Tool
TOOL_EXEC(执行 WASM Tool)
  ↓ Tool 结果
EVALUATE(判断是否继续)
  ↓ 需要更多 Tool
THINKING(继续推理)
  ↓ 完成
RESPOND(输出结果 + 变更摘要)
  ↓
IDLE
```

### 2.2 关键设计

- **多轮 Tool 调用**:单次用户输入可触发多次 Tool 调用(如先 parse 理解结构,再 apply_patch 修改,最后 render 渲染)
- **错误自修复**:Tool 返回错误时,错误信息回传给 LLM,LLM 据此调整并重试
- **上下文更新**:apply_patch 成功后,新 DSL 写入上下文,后续 Tool 基于新版本执行
- **防死循环**:最大迭代次数限制(默认 10)

### 2.3 实现位置

- 引擎:[src/agent/AgentLoop.ts](../src/agent/AgentLoop.ts)
- 上下文:[src/agent/context.ts](../src/agent/context.ts)
- Prompt:[src/agent/prompt.ts](../src/agent/prompt.ts)

## 3. Tool 设计

Agent 通过 6 个 Tool 操控 drawify-wasm。

### 3.1 Tool 清单

| Tool | WASM 函数 | 用途 | 状态 |
|------|-----------|------|------|
| render | render_with_options | 渲染 SVG/ASCII/JSON | 复用已有 |
| validate | validate | 校验 DSL | 复用已有 |
| parse | parse_to_json | 解析为 AST | 复用已有 |
| layout_catalog | layout_catalog | 查询布局算法 | 复用已有 |
| diff | diff_sources | 比较两版 DSL | 需新增 WASM 绑定 |
| apply_patch | apply_patch | 增量修改 DSL | 需新增 WASM 绑定 |

### 3.2 新增 WASM 绑定

Studio 需要在 drawify-wasm crate 新增 2 个导出:

#### diff_sources

```rust
#[wasm_bindgen]
pub fn diff_sources(old_source: &str, new_source: &str) -> String
```

复用 drawify-core 的 `diff::diff` 函数,比较两份 DSL 的 AST 差异,返回 `DiffResult` JSON。

#### apply_patch

```rust
#[wasm_bindgen]
pub fn apply_patch(source: &str, patch_json: &str) -> String
```

复用 drawify-core 的 `diff::apply_patch` 函数,应用 Change 列表到 AST,返回新 DSL 源码。

> 注意:apply_patch 需要将修改后的 AST 反序列化为 DSL 文本。当前 drawify-core 只有 DSL→AST 的单向解析,需新增 `ast_to_source` 能力。详见 [tasks.md](tasks.md) 的 P1 阶段。

### 3.3 Tool Schema

Tool Schema 定义在 [src/agent/tools.ts](../src/agent/tools.ts),遵循 OpenAI function-calling 格式,LLM 据此选择调用哪个 Tool。

## 4. 数据流

### 4.1 单轮对话(生成图表)

```
用户:"画一个微服务架构图"
  ↓
Agent Loop:
  1. LLM 推理 → 选择 render Tool
  2. 执行 render(source=生成的DSL, format=svg)
  3. WASM 返回 SVG
  4. LLM 判断完成 → 回复用户
  ↓
UI 更新:预览区显示 SVG,对话区显示 Agent 回复
```

### 4.2 多轮对话(增量修改)

```
用户:"给订单服务加一个 Redis 缓存"
  ↓
Agent Loop:
  1. LLM 推理 → 选择 apply_patch Tool
  2. 执行 apply_patch(source=当前DSL, patch=[新增redis实体+关系])
  3. WASM 返回新 DSL
  4. 上下文更新:source = 新 DSL
  5. LLM 推理 → 选择 render Tool
  6. 执行 render(source=新DSL, format=svg)
  7. WASM 返回 SVG
  8. LLM 推理 → 选择 diff Tool
  9. 执行 diff(old=旧DSL, new=新DSL)
  10. WASM 返回 DiffResult
  11. LLM 判断完成 → 回复用户
  ↓
UI 更新:预览区显示新 SVG,对话区显示变更摘要
```

### 4.3 错误自修复

```
Agent 生成 DSL → render 返回 E003 错误(引用了不存在的实体)
  ↓
LLM 根据错误信息 → 选择 apply_patch Tool(补上缺失的实体)
  ↓
重新 render → 成功
```

## 5. LLM 抽象层

### 5.1 Provider 支持

| Provider | 接入方式 | Tool-Calling |
|----------|---------|--------------|
| OpenAI | API Key + Bearer token | 原生支持 |
| Anthropic | API Key + x-api-key | tool_use 格式 |
| Ollama | localhost:11434 | OpenAI 兼容 |
| Custom | 任意 OpenAI 兼容端点 | 取决于实现 |

### 5.2 统一接口

所有 Provider 统一为 `LLMClient` 接口:

```typescript
interface LLMClient {
  chat(params: {
    messages: LLMMessage[];
    tools?: ToolSchema[];
  }): Promise<LLMResponse>;
}
```

Anthropic 的响应格式在 [src/lib/llm.ts](../src/lib/llm.ts) 中转换为统一的 `LLMResponse`。

### 5.3 配置

通过环境变量配置(见 `.env.example`),API Key 仅存 localStorage,不发送到任何第三方。

## 6. 与 drawify-core 的关系

Studio **不修改** drawify-core 的管线架构,只通过 WASM 消费其能力:

```
Studio 需要的 core 能力        现状        需要做的
──────────────────────────────────────────────────────
parse → AST                   已有         WASM 已导出
validate                      已有         WASM 已导出
render (svg/ascii/json)       已有         WASM 已导出
layout_catalog                已有         WASM 已导出
diff (两份 DSL 比较)           core 已有    需新增 WASM 导出
apply_patch                   core 已有    需新增 WASM 导出
ast_to_source (AST→DSL)       core 无      需在 core 新增
```

`ast_to_source` 是唯一需要在 core 层新增的能力,详见 [tasks.md](tasks.md)。

## 7. 安全性

- **API Key 本地存储**:LLM API Key 仅存浏览器 localStorage,不上传
- **无后端**:纯前端 WASM 应用,除 LLM API 外不与任何服务器通信
- **CSP 策略**:生产构建建议配置 Content-Security-Policy
- **输入校验**:DSL 经 drawify-core 校验,防止注入

## 8. 性能考量

- **WASM 单例**:drawify-wasm 模块全局单例,避免重复加载
- **对话历史压缩**:超过 20 条消息自动压缩,避免上下文过长
- **渲染防抖**:DSL 变更后防抖 300ms 再渲染
- **Tool 结果截断**:日志展示时截断过长结果
