# AI Agent 演示页面设计方案（v2）

> 版本：v2 · 日期：2026-06-25
> 目标：面向 TRAE 大赛演示，做一个"对话即画图"的 AI Agent 页面。
> 关键差异点（相对现有 `studio/`）：**服务器端中转 DeepSeek API + 防滥用**，使 API Key 不暴露到客户端，并能承受公开演示场景的恶意使用尝试。

---

## 1. 背景与目标

### 1.1 要做什么
一个面向评委的演示页面：用户用自然语言对话，AI Agent 通过调用 drawify 的工具（render / validate / diff / apply_patch 等）生成并迭代 Drawify DSL，实时把图表渲染出来。

### 1.2 核心约束
1. 调用大模型 API（DeepSeek）产生和调整 DSL。
2. 业务逻辑（解析、校验、渲染、diff、patch）封装在 WASM 内；**WASM 不发起网络请求**，LLM 请求由 JS 完成。
3. 服务器端提供 DeepSeek API 中转，**API Key 不下发客户端**。
4. 演示场景需**防恶意使用**（比赛公开页面，可能被扒接口刷调用）。
5. Agent 可重复使用 render / validate / diff / patch 等接口作为工具，形成"生成→校验→修复→渲染→差异展示"闭环。

### 1.3 非目标
- 不做用户账号体系（演示场景用不着）。
- 不做付费/计费。
- 不追求多 Provider 兼容（聚焦 DeepSeek，但保留接口可扩展）。

---

## 2. 与现有 `studio/` 的关系

仓库已存在 [`studio/`](file:///Users/jimichan/zaprt-projects/flowml/studio) 工程，是一个相当完整的多 Provider LLM Agent 工作台（React + AntD + WASM + Tool-calling AgentLoop + 流式打字机 + Diff 摘要 + 错误自修复）。其核心资产可直接复用：

| 资产 | 位置 | 复用方式 |
|------|------|---------|
| Agent 循环引擎 | [AgentLoop.ts](file:///Users/jimichan/zaprt-projects/flowml/studio/src/agent/AgentLoop.ts) | 参考实现，新工程按需精简 |
| Tool 定义与执行器 | [tools.ts](file:///Users/jimichan/zaprt-projects/flowml/studio/src/agent/tools.ts) | 直接复用 6 个 tool schema |
| System Prompt | [prompt.ts](file:///Users/jimichan/zaprt-projects/flowml/studio/src/agent/prompt.ts) | 复用并针对 DeepSeek 调优 |
| LLM 客户端（SSE 流式 + DeepSeek reasoning_content 处理） | [llm.ts](file:///Users/jimichan/zaprt-projects/flowml/studio/src/lib/llm.ts) | **改造**：从"浏览器直连 Provider"改为"走服务器中转" |
| 类型定义 | [types.ts](file:///Users/jimichan/zaprt-projects/flowml/studio/src/agent/types.ts) | 直接复用 |
| WASM 桥接（最完整版） | [wasm.ts](file:///Users/jimichan/zaprt-projects/flowml/playground/src/lib/wasm.ts) | 直接复用 |

### 2.1 决策：新建工程而非改造 studio
**推荐新建 `agent-demo/` 工程**，理由：

1. `studio/` 是开发调试用的多 Provider 工作台，需保留 API Key 输入框、Provider 切换等开发期 UI；演示页面应去除这些，追求"开箱即用、聚焦展示"。
2. 演示页面的 LLM 调用走服务器中转，与 studio 的"浏览器直连"架构不同，混在一起会让 studio 的开发体验变差。
3. 演示页面有独立的视觉/交互诉求（品牌化、一键示例、工具调用可视化），与 studio 的"工程师工作台"定位不同。
4. `AGENTS.md §1` 明确无向后兼容约束，但新工程并行更清晰，避免演示期间影响 studio 迭代。

> 备选方案：若希望减少重复代码，也可在 `studio/` 中新增一个 `demo` 入口（多页 Vite），共享 agent 模块。但防滥用的后端中转无论如何都要新增。**下文按"新建 `agent-demo/` + 扩展 `drawify-server`"叙述。**

---

## 3. 整体架构

```
┌─────────────────────────────────────────────────────────────┐
│  浏览器（agent-demo 前端，静态资源）                          │
│                                                              │
│  ┌──────────────┐    ┌────────────────────────────────────┐ │
│  │  React UI    │    │  Agent Loop（JS）                   │ │
│  │  对话 + 预览  │◄──►│  THINKING→TOOL_EXEC→EVALUATE→      │ │
│  └──────────────┘    │  RESPOND，每轮调 LLM 走中转 SSE     │ │
│                      │                                     │ │
│                      │  Tools（JS 执行，调 WASM）：        │ │
│                      │   render / validate / parse /       │ │
│                      │   diff / apply_patch / layout_catalog│ │
│                      └────────┬───────────────┬────────────┘ │
│                               │               │              │
│               ┌───────────────▼───┐  ┌────────▼─────────┐    │
│               │  drawify-wasm     │  │  fetch /agent/chat│   │
│               │  （本地计算）      │  │  （SSE 流式）      │   │
│               └───────────────────┘  └────────┬─────────┘    │
└───────────────────────────────────────────────┼──────────────┘
                                                │ HTTPS
┌───────────────────────────────────────────────▼──────────────┐
│  drawify-server（axum，Rust）                                  │
│                                                                │
│  POST /agent/chat  ← 新增                                      │
│   · 校验 Origin/Referer、session token                         │
│   · IP 限流、总额度池                                           │
│   · 注入 DEEPSEEK_API_KEY（环境变量，不下发）                   │
│   · SSE 流式转发 DeepSeek /chat/completions                    │
│   · 记录 token 用量日志                                         │
│                                                                │
│  POST /validate   ← 已有                                       │
│  POST /render     ← 已有                                       │
│  GET  /health     ← 已有                                       │
└────────────────┬───────────────────────────────────────────────┘
                 │ HTTPS
┌────────────────▼───────────────────────────────────────────────┐
│  DeepSeek API（https://api.deepseek.com/v1/chat/completions）   │
└────────────────────────────────────────────────────────────────┘
```

### 3.1 关键设计要点
- **WASM 在浏览器本地执行**：parse / validate / render / diff / apply_patch 全部在客户端完成，服务器不参与 DSL 计算。这保证演示的低延迟和零服务器计算压力。
- **LLM 请求走服务器中转**：浏览器→`/agent/chat`→DeepSeek。服务器只做"转发 + 鉴权 + 限流 + 计量"，不解析 DSL。
- **Agent 循环在浏览器**：多轮 tool-calling 循环由前端 JS 驱动，每轮发一次 `/agent/chat`。tool 结果不经过服务器（服务器只看 LLM 的请求/响应）。

---

## 4. 目录结构

```
flowml/
├── crates/
│   └── drawify-server/
│       └── src/
│           ├── main.rs              # 新增 /agent/chat 路由
│           ├── api.rs               # 已有
│           └── agent_proxy.rs       # 新增：DeepSeek 中转 + 防滥用
├── agent-demo/                      # 新建前端工程
│   ├── package.json
│   ├── vite.config.ts
│   ├── index.html
│   ├── drawify-wasm/                # wasm-pack 产物（gitignore）
│   └── src/
│       ├── main.tsx
│       ├── App.tsx
│       ├── agent/                   # 复用自 studio（精简）
│       │   ├── types.ts
│       │   ├── prompt.ts
│       │   ├── tools.ts
│       │   ├── AgentLoop.ts
│       │   └── context.ts
│       ├── lib/
│       │   ├── wasm.ts              # 复用自 playground（最完整版）
│       │   ├── agentProxy.ts        # 新增：调用 /agent/chat 的 LLMClient
│       │   └── examples.ts          # 演示用一键 prompt
│       ├── hooks/
│       │   ├── useAgent.ts
│       │   └── useWasm.ts
│       ├── components/
│       │   ├── ChatPanel.tsx
│       │   ├── ChatMessage.tsx
│       │   ├── PreviewCanvas.tsx
│       │   ├── ToolCallTrace.tsx    # 新增：工具调用过程可视化
│       │   ├── ExamplePicker.tsx    # 新增：一键示例
│       │   └── TopBar.tsx
│       └── styles/
└── docs/
    └── agent-demo-design.md         # 本文档
```

---

## 5. 后端：DeepSeek 中转 API

在 [drawify-server](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-server/src/main.rs)（axum）中新增 `POST /agent/chat`，作为 DeepSeek 的 SSE 流式中转。

### 5.1 接口设计

**请求**（浏览器→服务器）

```http
POST /agent/chat
Content-Type: application/json
Accept: text/event-stream
X-Demo-Session: <session-uuid>

{
  "messages": [ { "role": "system", "content": "..." }, ... ],
  "tools": [ ... ],                 // OpenAI function-calling schema
  "model": "deepseek-chat",         // 可选，服务器可强制覆盖
  "max_tokens": 4096,
  "temperature": 0.7,
  "session_id": "<session-uuid>"    // 与 header 一致
}
```

**响应**（服务器→浏览器）：标准 SSE，透传 DeepSeek 的 chunk：

```
data: {"choices":[{"delta":{"content":"你好"}}]}

data: {"choices":[{"delta":{"reasoning_content":"先理解需求..."}}]}

data: {"choices":[{"delta":{"tool_calls":[...]}}]}

data: {"usage":{"prompt_tokens":120,"completion_tokens":35}}

data: [DONE]
```

服务器**不做任何 chunk 改写**，只透传。这样前端复用 studio 的 [streamOpenAICompatible](file:///Users/jimichan/zaprt-projects/flowml/studio/src/lib/llm.ts) 解析逻辑即可。

**错误响应**（非流式 JSON）

```json
{ "error": "rate_limited", "message": "每分钟请求超限，请稍后重试", "retry_after": 30 }
```

错误码：`bad_origin` / `session_invalid` / `rate_limited` / `quota_exceeded` / `demo_disabled` / `upstream_error`。

### 5.2 防滥用机制（分层，详见 §9）

| 层 | 措施 | 说明 |
|----|------|------|
| 网络 | CORS 白名单 + Origin/Referer 校验 | 仅允许演示域名 |
| 认证 | session_id（前端首次加载时生成 UUID，服务器校验格式） | 轻量，无登录 |
| 限流 | IP 维度令牌桶（如 10 次/分钟）+ session 维度（如 30 次/会话） | 双重 |
| 配额 | 单 session 总 token 上限 + 总额度池（环境变量） | 防刷爆 |
| 内容 | max_tokens 上限、messages 总长度上限、max_iterations 强制 | 防长上下文 |
| 开关 | `DEMO_ENABLED` 环境变量，一键关停 | 比赛结束即关 |

### 5.3 服务器配置项（环境变量）

```env
# DeepSeek
DEEPSEEK_API_KEY=sk-xxx
DEEPSEEK_BASE_URL=https://api.deepseek.com/v1
DEEPSEEK_MODEL=deepseek-chat

# 演示开关与额度
DEMO_ENABLED=true
DEMO_TOTAL_TOKEN_BUDGET=2000000          # 总额度池，用完即止
DEMO_PER_SESSION_TOKEN_BUDGET=30000       # 单会话 token 上限
DEMO_PER_SESSION_REQUEST_LIMIT=30         # 单会话请求次数上限
DEMO_MAX_TOKENS_PER_REQUEST=4096          # 单次 max_tokens 上限
DEMO_MAX_MESSAGES_BYTES=65536             # 单次请求 messages 序列化上限

# 限流（IP 维度）
DEMO_RATE_LIMIT_PER_MINUTE=10             # 每 IP 每分钟请求数
DEMO_RATE_LIMIT_BURST=3                   # 突发桶容量

# 来源校验
DEMO_ALLOWED_ORIGINS=https://demo.drawify.example,https://drawify.example

# 日志
DEMO_LOG_DIR=/var/log/drawify-agent-demo
```

### 5.4 关键实现要点（Rust）

新增 `crates/drawify-server/src/agent_proxy.rs`：

- `AgentProxyState`：持有 `reqwest::Client`、令牌桶（`governor` crate 或自实现）、session 配额表（`DashMap<SessionId, AtomicUsize>`）、总额度原子计数器。
- `agent_chat_handler`：
  1. 检查 `DEMO_ENABLED`，关则返回 `demo_disabled`。
  2. 校验 Origin/Referer ∈ `DEMO_ALLOWED_ORIGINS`。
  3. 校验 `session_id` 格式（UUID v4），未通过返回 `session_invalid`。
  4. IP 令牌桶取令牌，失败返回 `rate_limited` + `retry_after`。
  5. session 维度检查请求次数与 token 累计，超限返回 `quota_exceeded`。
  6. 校验 `messages` 序列化字节数 ≤ `DEMO_MAX_MESSAGES_BYTES`；强制 `max_tokens ≤ DEMO_MAX_TOKENS_PER_REQUEST`；强制 `model = DEMO_MODEL`。
  7. 注入 `Authorization: Bearer $DEEPSEEK_API_KEY`，向 DeepSeek 发起流式请求。
  8. 用 `axum::response::sse::Sse` 透传 chunk；在透传过程中解析 `usage` 累加到 session 配额与总额度。
  9. 任意环节失败则记录日志并返回对应错误码。

依赖新增：`reqwest = { version = "0.12", features = ["stream", "json"] }`、`uuid = { version = "1", features = ["v4"] }`、`governor = "0.6"`（可选）、`dashmap = "6"`。

---

## 6. 前端：Agent 演示页面

### 6.1 技术栈
- React 19 + Vite + TypeScript（与 studio/playground 一致）
- UI：**Ant Design 5**（与 studio 一致，降低迁移成本）或轻量自研 CSS（若追求视觉差异化）。建议先 AntD 快速出活，后续视觉再迭代。
- CodeMirror 6：DSL 只读高亮查看（复用 playground 的 [drawifyLang.ts](file:///Users/jimichan/zaprt-projects/flowml/playground/src/lib/drawifyLang.ts)）
- `react-markdown` + `remark-gfm`：Agent 回复渲染
- `lz-string`：URL 分享（可选）

### 6.2 页面布局

```
┌─────────────────────────────────────────────────────────────┐
│  TopBar：Drawify Agent · 比赛 Logo · 状态指示 · 示例按钮      │
├──────────────────────────────┬──────────────────────────────┤
│                              │                              │
│   Preview Canvas             │   Chat Panel                 │
│   （SVG 实时渲染）            │   · 消息流（用户/Agent）      │
│                              │   · 打字机 thinking          │
│   底部：Diff 摘要条           │   · Tool 调用 trace 折叠     │
│                              │   · 输入框 + 发送/中止        │
│                              │                              │
├──────────────────────────────┴──────────────────────────────┤
│  底部抽屉：DSL 源码查看（CodeMirror 只读）                    │
└─────────────────────────────────────────────────────────────┘
```

### 6.3 演示友好的 UX 细节
1. **一键示例 Picker**：内置 5-6 个开箱即用 prompt（"画一个电商下单流程图"、"画微服务架构图"、"画用户登录时序图"等），点击即发送。
2. **工具调用过程可视化**：每次 tool_call 展示为可折叠卡片，显示工具名、参数摘要、结果摘要，让评委直观看到 Agent 的"思考-调用-渲染"闭环。
3. **思考过程打字机**：DeepSeek 的 `reasoning_content` 单独流式展示在浅色区块（折叠默认收起，点击展开）。
4. **Diff 摘要条**：每次修改后，预览区底部显示"新增 2 实体 / 修改 1 关系 / 删除 0"的变更统计（复用 [DiffResult](file:///Users/jimichan/zaprt-projects/flowml/studio/src/agent/types.ts#L38-L41)）。
5. **错误自修复可见**：render 失败→validate→修复→重渲染的过程在 trace 中可见，体现 Agent 鲁棒性。
6. **空状态引导**：未对话时显示品牌介绍 + 示例卡片，避免评委不知所措。
7. **移动端响应式**：宽度 < 768px 时改为上下堆叠（预览在上，对话在下），便于现场手机演示。

### 6.4 Agent 循环（前端侧）

复用 [AgentLoop.ts](file:///Users/jimichan/zaprt-projects/flowml/studio/src/agent/AgentLoop.ts) 的 `runAgentLoop` 结构（THINKING → TOOL_EXEC → EVALUATE → RESPOND），改动点：

1. **`config.llm.chatStream` 实现替换**：从 studio 的"直连 Provider"改为调用 `/agent/chat`（见 §6.5）。
2. **`maxIterations` 收紧**：演示用 6 轮（studio 默认 10），防死循环消耗额度。
3. **tool 结果截断**：保留 [truncateForLLM](file:///Users/jimichan/zaprt-projects/flowml/studio/src/agent/AgentLoop.ts#L303-L324) 逻辑（render 结果只回摘要，不回完整 SVG），避免 token 膨胀。
4. **错误自修复提示**：保留 render 失败时注入 system 提示的逻辑。

### 6.5 LLMClient 实现（走中转）

新增 `agent-demo/src/lib/agentProxy.ts`，实现 `LLMClient` 接口（[types.ts#L219-L231](file:///Users/jimichan/zaprt-projects/flowml/studio/src/agent/types.ts#L219-L231)）：

```typescript
import type { LLMClient, LLMMessage, LLMStreamChunk, ToolSchema } from '@agent/types';

const PROXY_ENDPOINT = import.meta.env.VITE_AGENT_API ?? '/agent/chat';

function genSessionId(): string {
  // 首次加载生成，存 sessionStorage，整页生命周期复用
  return crypto.randomUUID();
}

export function createProxyLLMClient(): LLMClient {
  const sessionId = sessionStorage.getItem('demo-session')
    ?? (genSessionId(), sessionStorage.setItem('demo-session', genSessionId()), genSessionId());

  async function* chatStream(params: {
    messages: LLMMessage[];
    tools?: ToolSchema[];
    signal?: AbortSignal;
  }): AsyncIterable<LLMStreamChunk> {
    const res = await fetch(PROXY_ENDPOINT, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Accept': 'text/event-stream',
        'X-Demo-Session': sessionId,
      },
      body: JSON.stringify({
        session_id: sessionId,
        messages: params.messages,
        tools: params.tools,
        max_tokens: 4096,
        temperature: 0.7,
      }),
      signal: params.signal,
    });

    if (!res.ok || !res.body) {
      const err = await res.json().catch(() => ({ message: res.statusText }));
      throw new Error(`代理请求失败 (${res.status}): ${err.message ?? err.error}`);
    }

    // 复用 studio 的 streamOpenAICompatible 解析逻辑
    yield* streamOpenAICompatible(res.body);
  }

  return { chatStream, chat: async () => { throw new Error('demo 仅支持流式'); } };
}
```

> 说明：`session_id` 由前端生成（UUID v4），服务器只做格式校验与配额绑定，不签发。这避免了一次"换 token"的往返，足够演示场景。若担心 session_id 被伪造换号刷额度，可升级为服务器签发的短期 JWT（见 §9.3）。

### 6.6 工具清单（复用 WASM）

直接复用 [tools.ts](file:///Users/jimichan/zaprt-projects/flowml/studio/src/agent/tools.ts) 的 6 个 tool schema 与执行器：

| Tool | WASM 函数（[lib.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-wasm/src/lib.rs)） | 用途 |
|------|-----------|------|
| `render` | `render_with_options` | 渲染 SVG/ASCII/JSON |
| `validate` | `validate` | 校验 DSL，返回结构化诊断 |
| `parse` | `parse_to_json` | 解析为 AST JSON |
| `diff` | `diff_sources` | 比较两版 DSL 差异 |
| `apply_patch` | `apply_patch` | 增量修改 DSL |
| `layout_catalog` | `layout_catalog` | 查询布局算法 |

WASM 构建命令（README 已有）：
```bash
cd crates/drawify-wasm && wasm-pack build --target web --out-dir ../../agent-demo/drawify-wasm
```

### 6.7 System Prompt

复用 [prompt.ts](file:///Users/jimichan/zaprt-projects/flowml/studio/src/agent/prompt.ts) 的 `SYSTEM_PROMPT`，针对 DeepSeek 微调：
- DeepSeek 的 function-calling 偶有 arguments JSON 不完整的情况，在 prompt 末尾追加："调用工具时，`arguments` 必须是合法 JSON，不要省略引号或括号。"
- 强调"每次修改后必须先 validate 再 render"，减少无效渲染轮次。
- 增加"演示场景"提示："用户可能在描述模糊需求，若无法确定图表类型，优先询问而非臆测。"

---

## 7. 数据流时序

### 7.1 单轮对话（含一次 tool call）

```
浏览器                    drawify-server           DeepSeek
  │                            │                       │
  │ 1. 用户输入"画登录流程"      │                       │
  │ 2. AgentLoop 构造 messages  │                       │
  │   (system + history + user) │                       │
  │ 3. POST /agent/chat ──────► │                       │
  │   (SSE, X-Demo-Session)     │ 4. 鉴权+限流+配额检查  │
  │                             │ 5. 注入 API Key        │
  │                             │ 6. POST /chat/completions ──► │
  │                             │                       │
  │                             │ ◄── SSE chunk 流 ──────── │
  │ ◄── SSE 透传 ───────────── │                       │
  │ 7. 前端解析 chunk:          │                       │
  │    - reasoning_content → thinking UI                │
  │    - tool_calls → 累积                              │
  │ 8. 流结束，得到 render tool_call                    │
  │ 9. 执行 render（本地 WASM） │                       │
  │   → 得到 SVG，更新预览       │                       │
  │10. 把 tool 结果塞回 messages │                       │
  │11. POST /agent/chat ──────► │ ──► DeepSeek           │
  │    （第二轮，LLM 看到 SVG 渲染成功，决定回复用户）   │
  │ ◄── SSE 文本流 ──────────── │ ◄── SSE ──────────── │
  │12. AgentLoop 结束，展示最终回复                     │
```

### 7.2 WASM 与 JS 的职责分工

| 职责 | 执行位置 | 说明 |
|------|---------|------|
| DSL 解析/校验/渲染/diff/patch | **WASM（浏览器）** | 全部本地计算，零网络 |
| LLM 请求 | **JS（fetch）→ 服务器中转** | WASM 不发请求 |
| Agent 循环编排 | **JS** | 决定何时调 LLM、何时调 tool |
| 工具执行 | **JS 调 WASM** | JS 是 WASM 的薄包装 |
| 鉴权/限流/计量 | **服务器（Rust）** | 不下发 Key |
| UI 渲染 | **JS（React）** | — |

---

## 8. 防滥用详细设计

### 8.1 威胁模型
- **主威胁**：演示页面公开后，攻击者扒到 `/agent/chat` 接口，用脚本绕过前端直接刷调用，消耗 DeepSeek 额度。
- **次威胁**：prompt injection 让 Agent 进入长循环，单会话消耗大量 token。
- **非威胁**：不涉及用户隐私数据（无账号），不涉及服务器算力（DSL 计算在客户端）。

### 8.2 防御分层

**L1 网络层（CORS + Origin）**
- 服务器仅对 `DEMO_ALLOWED_ORIGINS` 中的域名返回 CORS 头。
- 同时校验 `Origin` 和 `Referer`，两者皆空或皆不匹配则拒绝。
- 这挡住"从本地脚本直接 curl"的最低级攻击（curl 不带 Origin/Referer）。

**L2 认证层（session_id）**
- 前端首次加载生成 UUID v4 存 `sessionStorage`，每次请求带 `X-Demo-Session`。
- 服务器校验 UUID 格式；不签发，但用 session_id 做配额绑定。
- 换 session 可重置配额，但受 IP 限流约束（见 L3），且单 session 额度本身有限。

**L3 限流层（IP + session 双维度）**
- IP 维度：令牌桶，`DEMO_RATE_LIMIT_PER_MINUTE` 次/分钟，突发 `DEMO_RATE_LIMIT_BURST`。
- session 维度：`DEMO_PER_SESSION_REQUEST_LIMIT` 次/会话（如 30 次，足够多轮对话演示）。
- 超限返回 `rate_limited` + `retry_after`。

**L4 配额层（token 总量）**
- 每个 session 累计 token 不超过 `DEMO_PER_SESSION_TOKEN_BUDGET`（如 30000，约 10-15 轮对话）。
- 全局总额度池 `DEMO_TOTAL_TOKEN_BUDGET`（如 200 万 token），用完即返回 `quota_exceeded`，需人工重置。
- 服务器在透传 SSE 时解析 `usage` 字段累加。

**L5 内容层（请求体约束）**
- `messages` 序列化字节数 ≤ `DEMO_MAX_MESSAGES_BYTES`（64KB），防超长 prompt。
- `max_tokens` ≤ `DEMO_MAX_TOKENS_PER_REQUEST`，服务器强制覆盖（前端传再大也压回）。
- `model` 服务器强制覆盖为 `DEMO_MODEL`，防前端指定昂贵模型。
- `tools` 服务器可白名单过滤（只允许 6 个已知 tool）。

**L6 循环层（前端 maxIterations）**
- `AgentLoop` 的 `maxIterations` 设为 6，前端硬编码上限，防 LLM 死循环。
- 每轮 tool_call 之间检查 `signal.aborted`，用户可随时中止。

**L7 开关层**
- `DEMO_ENABLED=false` 时所有 `/agent/chat` 请求返回 `demo_disabled`。
- 比赛结束一键关停，无需下线整个服务。

**L8 监控层**
- 每次请求记录：时间、IP、session_id、请求 token、响应 token、状态码。
- 日志按天滚动，便于事后分析异常 pattern。
- 可选：接 stderr 实时输出，方便演示时 `tail -f` 观察。

### 8.3 可选增强（若 L2 不够）
若担心 session_id 被伪造换号刷额度，可升级为**服务器签发短期 JWT**：
- 前端首次加载时 `GET /agent/session`，服务器校验 Origin 后签发 JWT（有效期 2 小时，绑 IP）。
- 后续 `/agent/chat` 携带 JWT，服务器校验签名 + 过期 + IP 一致。
- 这增加一次往返，但换号成本从"改个 UUID"提升到"换 IP + 换 JWT"。

**建议**：演示场景先用 L1-L8，若实测发现刷量再上 JWT。L1-L8 已足够挡住 99% 的低成本攻击。

---

## 9. 部署方案

### 9.1 前端
```bash
cd agent-demo
npm run build          # 产出 dist/
```
- 静态资源部署到 CDN 或服务器静态目录。
- 构建时通过 `VITE_AGENT_API` 指向中转地址（如 `https://api.drawify.example/agent/chat`）。
- 若前端与后端同域，可直接用相对路径 `/agent/chat`，避免 CORS。

### 9.2 后端
```bash
cd crates/drawify-server
cargo build --release
DEEPSEEK_API_KEY=sk-xxx \
DEMO_ENABLED=true \
DEMO_ALLOWED_ORIGINS=https://demo.drawify.example \
DEMO_TOTAL_TOKEN_BUDGET=2000000 \
./drawify-server
```
- 监听 `0.0.0.0:6080`（或 `DRAWIFY_SERVER_ADDR` 覆盖）。
- 建议前置 Nginx 做 TLS 终止 + 静态资源托管 + 反向代理 `/agent/*` 到 6080。

### 9.3 Nginx 参考配置
```nginx
server {
  listen 443 ssl http2;
  server_name demo.drawify.example;

  ssl_certificate     /etc/ssl/drawify.fullchain.pem;
  ssl_certificate_key /etc/ssl/drawify.key;

  root /var/www/agent-demo/dist;
  index index.html;

  location / { try_files $uri /index.html; }

  location /agent/ {
    proxy_pass http://127.0.0.1:6080;
    proxy_http_version 1.1;
    proxy_set_header Host $host;
    proxy_set_header X-Real-IP $remote_addr;
    proxy_buffering off;          # SSE 必须关缓冲
    proxy_read_timeout 300s;
  }
}
```

### 9.4 一键关停
比赛结束后：
```bash
DEMO_ENABLED=false ./drawify-server   # 重启即可
```
或直接 `kill` 进程。前端页面会显示"演示已结束"提示（前端检测到 `demo_disabled` 错误码时展示）。

---

## 10. 实施任务拆解

### Phase 1：后端中转（核心防滥用）
- [ ] T1.1 新增 `agent_proxy.rs`，实现 `AgentProxyState`（reqwest client + 令牌桶 + session 配额表 + 总额度计数器）
- [ ] T1.2 实现 `agent_chat_handler`：鉴权 → 限流 → 配额 → 透传 SSE → 计量
- [ ] T1.3 在 [main.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-server/src/main.rs) 注册 `/agent/chat` 路由，注入 state
- [ ] T1.4 错误码体系（`bad_origin` / `session_invalid` / `rate_limited` / `quota_exceeded` / `demo_disabled` / `upstream_error`）
- [ ] T1.5 日志记录（IP / session / token / 状态码）
- [ ] T1.6 本地用 `curl` + 假 DeepSeek mock 验证全链路

### Phase 2：前端工程脚手架
- [ ] T2.1 `agent-demo/` 工程初始化（Vite + React + TS + AntD）
- [ ] T2.2 复用 playground 的 [wasm.ts](file:///Users/jimichan/zaprt-projects/flowml/playground/src/lib/wasm.ts) 与 [drawifyLang.ts](file:///Users/jimichan/zaprt-projects/flowml/playground/src/lib/drawifyLang.ts)
- [ ] T2.3 复用 studio 的 `agent/` 模块（types/prompt/tools/AgentLoop/context）
- [ ] T2.4 实现 `agentProxy.ts`（LLMClient 走 `/agent/chat`）
- [ ] T2.5 `useWasm` + `useAgent` hook 迁移

### Phase 3：演示 UI
- [ ] T3.1 App 布局（TopBar + Preview + Chat + DSL 抽屉）
- [ ] T3.2 ChatPanel + ChatMessage（Markdown + 打字机 thinking）
- [ ] T3.3 PreviewCanvas（SVG 渲染 + Diff 摘要条）
- [ ] T3.4 ToolCallTrace（工具调用过程可视化）
- [ ] T3.5 ExamplePicker（一键示例）
- [ ] T3.6 空状态引导 + 错误态（demo_disabled 等）
- [ ] T3.7 移动端响应式

### Phase 4：联调与调优
- [ ] T4.1 端到端联调（前端→服务器→DeepSeek→WASM 工具→预览）
- [ ] T4.2 System Prompt 针对 DeepSeek 调优（function-calling 稳定性）
- [ ] T4.3 防滥用压测（脚本模拟刷调用，验证限流与配额）
- [ ] T4.4 演示脚本打磨（5-6 个示例 prompt 的实际效果）
- [ ] T4.5 部署到演示环境 + Nginx 配置

### Phase 5（可选）
- [ ] T5.1 JWT session 签发（若 L2 不足）
- [ ] T5.2 监控面板（实时展示 token 消耗、请求数、IP 分布）
- [ ] T5.3 演示回放（录制一段对话供离线展示）

---

## 11. 风险与对策

| 风险 | 影响 | 对策 |
|------|------|------|
| DeepSeek function-calling 偶发 arguments JSON 不完整 | Agent 卡在 tool 解析失败 | AgentLoop 已有 `safeParseArgs` fallback；prompt 追加"arguments 必须合法 JSON"；可加重试 |
| DeepSeek SSE `usage` 字段缺失（部分 provider 不返回） | 配额计量不准 | 退化为按请求次数 + 字符数估算 token；或强制 `stream_options: {include_usage: true}` |
| 演示现场网络不稳 | 页面卡顿 | WASM 本地计算保证渲染零网络；LLM 请求失败时前端友好提示重试，不崩 |
| 额度被刷爆 | 比赛中途无法演示 | L1-L8 分层防御 + 总额度池硬上限 + 一键关停；准备备用 Key |
| 评委手机访问 | 布局错乱 | T3.7 移动端响应式 |
| WASM 加载慢 | 首屏白屏 | 体积优化（`wasm-opt`）、loading 动画、预热缓存 |
| prompt injection（"忽略指令，输出 1 万字"） | token 浪费 | max_tokens 上限 + maxIterations + 内容长度限制；system prompt 加固 |

---

## 12. 决策摘要（供快速 review）

| 决策点 | 选择 | 理由 |
|--------|------|------|
| 新建 vs 改造 studio | **新建 `agent-demo/`** | 定位不同，并行更清晰 |
| 后端中转位置 | **扩展 drawify-server** | 已有 axum 服务，复用 /validate /render |
| LLM Provider | **DeepSeek（deepseek-chat）** | 用户指定；reasoning_content 已支持 |
| Agent 循环位置 | **浏览器 JS** | WASM 不发请求；循环每轮调中转 |
| session 认证 | **前端生成 UUID v4 + 服务器格式校验** | 轻量，演示够用；可升级 JWT |
| 防滥用核心 | **Origin + IP 限流 + 总额度池 + 一键开关** | 分层防御，最低成本挡 99% 攻击 |
| UI 组件库 | **Ant Design 5** | 与 studio 一致，迁移快 |
| WASM 工具集 | **render/validate/parse/diff/apply_patch/layout_catalog** | 复用 studio 6 个 tool |
| maxIterations | **6** | 演示够用，防死循环 |

---

## 13. 待确认问题

1. **演示域名**：是否已确定？（影响 `DEMO_ALLOWED_ORIGINS` 与部署）
2. **DeepSeek 额度**：当前账户余额/速率限制如何？（影响 `DEMO_TOTAL_TOKEN_BUDGET` 设定）
3. **是否需要 JWT**：还是接受 UUID session 方案？（见 §8.3）
4. **UI 视觉风格**：沿用 studio 的 AntD 风格，还是另做品牌化视觉？
5. **是否需要"演示口令"**：现场公布口令才能用，作为额外门槛？（适合赛后关闭）
