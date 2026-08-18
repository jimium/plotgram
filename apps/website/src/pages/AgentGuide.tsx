import DocPage, { DOCS_SIDEBAR } from '../components/DocPage';
import CodeBlock from '../components/CodeBlock';

const sidebar = DOCS_SIDEBAR.map((section) => ({
  ...section,
  items: section.items.map((item) => ({
    ...item,
    active: item.label === 'Agent 集成指南',
  })),
}));

const curlCode = `curl -X POST https://api.tautcore.com/v1/render \\
  -H "Content-Type: application/json" \\
  -d '{
    "source": "diagram flowchart { title: \\"示例\\" entity[a] start \\"开始\\" entity[b] end \\"结束\\" a -> b }",
    "options": { "theme": "clean-light" }
  }'`;

const wasmCode = `import init, { render } from './tautcore_wasm.js';
await init();
const svg = render(source, { theme: 'clean-light' });`;

const systemPromptCode = `你是一个图表生成助手。你必须使用 Tautcore 语法输出图表代码。

## 语法规则（严格遵守）
1. 始终以 diagram 开头声明图表类型：diagram flowchart { ... }
2. 支持的图表类型：flowchart, sequence, architecture, state, er, mindmap
3. 实体定义格式：entity[id] type "显示名称" { 属性 }
4. 仅三种箭头：
   ->  实线箭头：主动调用/请求/数据流
   --> 虚线箭头：响应/返回/事件通知
   <-> 双向箭头：双向通信/实时连接
5. 用 config { direction: left-to-right } 设置布局方向
6. 用语义角色命名实体类型：start, end, process, decision, database, service, gateway, user, browser, cache, server, client, api, auth

## 输出要求
- 只输出 Tautcore 代码，不要输出解释文字
- 代码放在 \`\`\`tautcore 代码块中
- 不要添加任何 Markdown 格式外的内容`;

const errorJsonCode = `{
  "errors": [
    {
      "code": "E003",
      "message": "Undefined entity reference: 'ordr_svc'",
      "line": 8,
      "column": 5,
      "context": "ordr_svc -> db \\"查询\\"",
      "suggestion": "Did you mean 'order_svc'? The entity 'order_svc' is defined at line 4."
    }
  ],
  "valid": false
}`;

const errorHandlerCode = `async function generateWithRetry(userPrompt: string, maxRetries = 2) {
  let source = await llm.generate(buildPrompt(userPrompt));

  for (let attempt = 0; attempt <= maxRetries; attempt++) {
    const result = await validate(source);

    if (result.valid) {
      return render(source);
    }

    if (attempt === maxRetries) break;

    const errorMsg = result.errors
      .map(e => \`行\${e.line}: \${e.message}。建议：\${e.suggestion}\`)
      .join('\\n');

    source = await llm.retry(
      \`生成的代码有误，请修复：\\n\${errorMsg}\\n\\n原代码：\\n\${source}\`
    );
  }

  throw new Error('图表生成失败，请检查描述后重试');
}`;

const patchJsonCode = `{
  "patches": [
    {
      "op": "update_entity",
      "id": "db",
      "changes": {
        "label": "PostgreSQL 主库",
        "semantic": "postgresql",
        "type": "database"
      }
    },
    {
      "op": "add_entity",
      "entity": {
        "id": "redis_cache",
        "type": "cache",
        "label": "Redis 缓存"
      }
    },
    {
      "op": "add_relation",
      "from": "order_svc",
      "to": "redis_cache",
      "arrow": "->",
      "label": "缓存查询"
    }
  ]
}`;

export default function AgentGuide() {
  return (
    <DocPage
      title="🤖 AI Agent 集成指南"
      description="把 Tautcore 嵌入你的 LLM 应用 / AI Agent，让 AI 自动生成、修改、修复图表。"
      sidebar={sidebar}
    >
      <div className="callout info">
        <div className="callout-icon">ℹ️</div>
        <div className="callout-body">
          <strong>AI 原生设计</strong>
          <p>Tautcore 的核心设计目标就是 AI 原生。从语法设计（极简规则、无语义歧义）到错误模型（结构化JSON含修复建议）再到操作范式（AST Diff &amp; Patch），每一处都为 Agent 工作流优化。</p>
        </div>
      </div>

      <h2>为什么 AI 需要专门的图表语言</h2>
      <p>传统图表语言（如 Mermaid）面向人类手写设计，语法体系庞大、隐式规则繁多，LLM 生成时容易出错，且错误难以自修复。</p>
      <p>具体来说，AI 生成图表面临三大核心痛点：</p>
      <ul>
        <li><strong>语法变体过多</strong>：仅箭头就有 10+ 种写法，LLM 容易混淆导致生成失败</li>
        <li><strong>错误反馈模糊</strong>：文本错误信息或静默失败，Agent 无法定位问题，自我修复极其困难</li>
        <li><strong>增量修改代价高</strong>：纯文本输出没有结构语义，微小修改也需要完整重新生成，容易引入新错误</li>
      </ul>

      <table>
        <thead>
          <tr>
            <th>对比维度</th>
            <th>Mermaid</th>
            <th>Tautcore</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <td>语法规则数量</td>
            <td>多（10+种箭头）</td>
            <td>少（3种箭头）</td>
          </tr>
          <tr>
            <td>隐式规则</td>
            <td>大量</td>
            <td>零</td>
          </tr>
          <tr>
            <td>错误反馈</td>
            <td>文本/静默失败</td>
            <td>结构化JSON</td>
          </tr>
          <tr>
            <td>自我修复支持</td>
            <td>困难</td>
            <td>一次重试修复</td>
          </tr>
          <tr>
            <td>增量修改</td>
            <td>不支持</td>
            <td>AST Patch</td>
          </tr>
        </tbody>
      </table>

      <h2>快速集成：三行代码调用 API</h2>
      <p>Tautcore 提供 HTTP API 和 WASM 两种集成方式，你可以根据部署场景选择：HTTP API 适合服务端渲染，WASM 适合浏览器端零延迟渲染。</p>

      <h3>HTTP API 渲染（curl 示例）</h3>
      <CodeBlock code={curlCode} language="bash" title="curl" />

      <h3>WASM / 浏览器端集成（TypeScript）</h3>
      <CodeBlock code={wasmCode} language="typescript" title="wasm-render.ts" />

      <h2>Prompt Engineering 最佳实践</h2>

      <h3>推荐 System Prompt 模板</h3>
      <p>一个好的 System Prompt 是生成质量的关键。以下是我们经过大量测试优化的模板：</p>
      <CodeBlock code={systemPromptCode} language="text" title="system-prompt.txt" />

      <h3>Prompt 设计原则</h3>
      <div className="doc-feature-grid">
        <div className="doc-feature-card">
          <div className="icon">🎯</div>
          <h4>明确图表类型</h4>
          <p>始终在 prompt 开头指定图表类型（流程图、时序图、架构图等），让 LLM 选择正确的语法子集。</p>
        </div>
        <div className="doc-feature-card">
          <div className="icon">📐</div>
          <h4>指定布局方向</h4>
          <p>流程图用 left-to-right（从左到右），层次结构图用 top-to-bottom（从上到下），避免布局混乱。</p>
        </div>
        <div className="doc-feature-card">
          <div className="icon">🏷️</div>
          <h4>用语义角色命名</h4>
          <p>按实体的角色命名（如"订单服务"而非"S1"），帮助 LLM 自动选择正确的 type 和图标。</p>
        </div>
        <div className="doc-feature-card">
          <div className="icon">✏️</div>
          <h4>少即是多</h4>
          <p>不要过度指定坐标和位置细节，让布局引擎自动处理，减少约束冲突导致的渲染失败。</p>
        </div>
      </div>

      <div className="callout tip">
        <div className="callout-icon">💡</div>
        <div className="callout-body">
          <strong>Few-shot 技巧</strong>
          <p>Few-shot 示例比长篇规则更有效。在 prompt 中附上 1-2 个简单示例，生成质量会显著提升。</p>
        </div>
      </div>

      <h2>结构化错误处理与自修复</h2>
      <p>Tautcore 的 validate API 返回结构化的 JSON 错误对象，包含错误位置、上下文和修复建议，让 Agent 可以精确地进行自我修复，而不需要重新生成全部内容。</p>
      <p>每个错误对象包含以下字段：</p>
      <ul>
        <li><code>code</code>：错误码，便于程序判断错误类型</li>
        <li><code>message</code>：人类可读的错误描述</li>
        <li><code>line</code> / <code>column</code>：错误在源码中的精确位置</li>
        <li><code>context</code>：错误所在行的源代码片段</li>
        <li><code>suggestion</code>：具体的修复建议（如变量名拼写纠正）</li>
      </ul>

      <CodeBlock code={errorJsonCode} language="json" title="error-response.json" />

      <h3>自修复工作流</h3>
      <div className="steps">
        <div className="step">
          <div className="step-number">1</div>
          <div className="step-body">
            <h3>首次生成</h3>
            <p>构造 System Prompt + 用户自然语言描述，调用 LLM 生成 Tautcore 源码。</p>
          </div>
        </div>
        <div className="step">
          <div className="step-number">2</div>
          <div className="step-body">
            <h3>检测错误</h3>
            <p>将生成的源码传入 validate API，检查语法和语义错误。如果 valid 为 true，直接进入渲染步骤。</p>
          </div>
        </div>
        <div className="step">
          <div className="step-number">3</div>
          <div className="step-body">
            <h3>自动修复重试</h3>
            <p>将错误信息（含 suggestion 字段）拼接到上下文中，让 LLM 针对性修复错误后重试。</p>
            <CodeBlock code={errorHandlerCode} language="typescript" title="error-handler.ts" />
          </div>
        </div>
      </div>

      <div className="callout warning">
        <div className="callout-icon">⚠️</div>
        <div className="callout-body">
          <strong>重试次数限制</strong>
          <p>实践中，超过 90% 的语法错误可以通过一次自我修复解决。如果两次重试后仍然失败，应将错误信息返回给用户，而不是无限重试。</p>
        </div>
      </div>

      <h2>AST Diff &amp; Patch：增量修改</h2>
      <p>当用户需要对已有图表进行小修改时（如改标签、加节点、换箭头），重新生成整张图既浪费 token 又容易引入新错误。Tautcore 提供 Patch API，支持原子化的增量修改。</p>
      <p>例如用户说："把数据库改成 PostgreSQL 语义，并加上 Redis 缓存层"，Agent 不需要重新生成整个图表，只需要发送一组 Patch 操作：</p>

      <CodeBlock code={patchJsonCode} language="json" title="patch-request.json" />

      <div className="callout info">
        <div className="callout-icon">🔧</div>
        <div className="callout-body">
          <strong>Patch 操作类型</strong>
          <p>Patch 支持 add_entity、update_entity、add_relation、delete_entity 等原子操作，Agent 可组合使用实现复杂修改而不破坏其他部分。</p>
        </div>
      </div>

      <h2>完整 Agent 工作流建议</h2>
      <p>将以上能力组合起来，推荐的 AI Agent 图表生成工作流如下：</p>
      <ol>
        <li>接收用户自然语言描述</li>
        <li>构造 System Prompt + 用户描述 + Few-shot 示例</li>
        <li>调用 LLM 生成 Tautcore 源码</li>
        <li>调用 validate API 检查语法</li>
        <li>如有错误，将错误信息（含 suggestion）加入上下文重试（最多2次）</li>
        <li>调用 render API 生成 SVG</li>
        <li>返回给用户；后续修改使用 Patch API 增量更新</li>
      </ol>

      <h2>下一步</h2>
      <div className="quick-links">
        <a href="/docs/getting-started/" className="quick-link-card">
          <div className="ql-icon">🚀</div>
          <h4>快速上手</h4>
          <p>5 分钟学会 Tautcore 基础语法</p>
          <span className="ql-arrow">→</span>
        </a>
        <a href="/docs/how-it-works/" className="quick-link-card">
          <div className="ql-icon">🔧</div>
          <h4>技术揭秘</h4>
          <p>深入了解渲染引擎设计与实现</p>
          <span className="ql-arrow">→</span>
        </a>
      </div>
    </DocPage>
  );
}
