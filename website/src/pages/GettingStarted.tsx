import DocPage, { DOCS_SIDEBAR } from '../components/DocPage';
import CodeBlock from '../components/CodeBlock';

const sidebar = DOCS_SIDEBAR.map((section) => ({
  ...section,
  items: section.items.map((item) => ({
    ...item,
    active: item.label === '快速上手',
  })),
}));

const helloWorldCode = `diagram flowchart {
    title: "Hello Plotgram"
    config { direction: left-to-right }
    entity[start] start "开始"
    entity[process] step "处理"
    entity[end] end "结束"
    start -> step "第一步"
    step -> end "完成"
}`;

const semanticIconCode = `diagram flowchart {
    title: "Hello Plotgram"
    config { direction: left-to-right }
    entity[start] start "开始"
    entity[process] step "处理" { type: service }
    entity[db] database "数据库" { type: database }
    entity[end] end "结束"
    start -> step "请求"
    step -> db "查询"
    db --> step "结果"
    step -> end "完成"
}`;

const ENTITY_TYPES = [
  { type: 'start', icon: '🟢', desc: '流程起点，圆角矩形表示' },
  { type: 'end', icon: '🔴', desc: '流程终点，加粗圆角矩形' },
  { type: 'process', icon: '⚙️', desc: '处理步骤，标准矩形' },
  { type: 'decision', icon: '🔷', desc: '判断/分支节点，菱形' },
  { type: 'database', icon: '🗄️', desc: '数据库存储，圆柱形' },
  { type: 'service', icon: '🔧', desc: '微服务/API 服务，齿轮图标' },
  { type: 'gateway', icon: '🚪', desc: 'API 网关/入口，门形图标' },
  { type: 'user', icon: '👤', desc: '用户/参与者，人形图标' },
  { type: 'cache', icon: '⚡', desc: '缓存层（Redis 等），闪电图标' },
  { type: 'browser', icon: '🌐', desc: '浏览器/前端客户端' },
  { type: 'server', icon: '🖥️', desc: '服务器节点' },
  { type: 'client', icon: '💻', desc: '客户端应用' },
  { type: 'api', icon: '🔌', desc: '外部 API 接口' },
  { type: 'auth', icon: '🔐', desc: '认证/授权服务' },
];

export default function GettingStarted() {
  return (
    <DocPage
      title="5 分钟快速上手"
      description="从零开始，5 分钟内用 Plotgram 画出你的第一张图。"
      sidebar={sidebar}
    >
      <h2>三种使用方式</h2>
      <p>Plotgram 提供多种接入方式，根据你的场景选择最合适的一种开始体验。</p>
      <div className="quick-links">
        <a href="/playground/" className="quick-link-card">
          <div className="ql-icon">🌐</div>
          <h4>浏览器 Playground</h4>
          <p>零安装，打开即用，实时预览</p>
          <span className="ql-arrow">→</span>
        </a>
        <div className="quick-link-card" style={{ opacity: 0.6, cursor: 'not-allowed' }}>
          <div className="ql-icon">💻</div>
          <h4>CLI 命令行</h4>
          <p>本地渲染，支持批量处理和 CI 集成</p>
          <span className="ql-arrow" style={{ color: 'var(--text-muted)' }}>→</span>
        </div>
        <div className="quick-link-card" style={{ opacity: 0.6, cursor: 'not-allowed' }}>
          <div className="ql-icon">🧩</div>
          <h4>WASM / API</h4>
          <p>嵌入你的前端或后端服务</p>
          <span className="ql-arrow" style={{ color: 'var(--text-muted)' }}>→</span>
        </div>
      </div>

      <h2>Hello World：第一个流程图</h2>
      <p>让我们通过一个简单的三步教程，快速体验 Plotgram 的核心语法。</p>
      <div className="steps">
        <div className="step">
          <div className="step-number">1</div>
          <div className="step-body">
            <h3>打开 Playground</h3>
            <p>
              访问 <a href="/playground/">Playground</a>，在左侧编辑器中输入以下代码，右侧会实时渲染出流程图：
            </p>
            <CodeBlock code={helloWorldCode} title="hello.pgm" />
          </div>
        </div>
        <div className="step">
          <div className="step-number">2</div>
          <div className="step-body">
            <h3>添加语义图标</h3>
            <p>
              给实体添加 <code>type</code> 属性，Plotgram 会自动选择合适的语义图标和形状。比如数据库会显示为圆柱形，服务会显示齿轮图标：
            </p>
            <CodeBlock code={semanticIconCode} title="hello-semantic.pgm" />
            <p style={{ marginTop: 12 }}>
              注意我们还使用了 <code>--&gt;</code> 虚线箭头表示「响应/返回」的数据流方向。
            </p>
          </div>
        </div>
        <div className="step">
          <div className="step-number">3</div>
          <div className="step-body">
            <h3>试试复杂示例</h3>
            <p>
              当你掌握了基础语法后，可以前往 <a href="/showcase/">示例画廊</a> 浏览 70+ 种图表示例，涵盖流程图、时序图、架构图、状态机、ER 图、思维导图等多种类型。
            </p>
          </div>
        </div>
      </div>

      <h2>核心语法速览</h2>
      <p>Plotgram 语法设计遵循「最少概念」原则——只有 5 个核心元素，就能描述绝大多数技术图表。</p>
      <table>
        <thead>
          <tr>
            <th>元素</th>
            <th>语法</th>
            <th>说明</th>
            <th>示例</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <td><strong>diagram 声明</strong></td>
            <td><code>diagram 类型 {'{'}</code></td>
            <td>声明图表类型，支持 flowchart/sequence/architecture/state/er/mindmap</td>
            <td><code>diagram flowchart {'{'}</code></td>
          </tr>
          <tr>
            <td><strong>entity 实体</strong></td>
            <td><code>entity[id] type "标签" {'{'} ... {'}'}</code></td>
            <td>定义图中的节点，id 用于引用，type 决定图标形状</td>
            <td><code>entity[db] database "订单库" {'{'} type: database {'}'}</code></td>
          </tr>
          <tr>
            <td><strong>arrow 箭头</strong></td>
            <td><code>a -{'>'} b "标签"</code></td>
            <td>连接两个实体，共 3 种箭头表达不同语义</td>
            <td><code>client -{'>'} gw "请求"</code></td>
          </tr>
          <tr>
            <td><strong>config 配置</strong></td>
            <td><code>config {'{'} ... {'}'}</code></td>
            <td>设置布局方向、主题等渲染参数</td>
            <td><code>config {'{'} direction: left-to-right {'}'}</code></td>
          </tr>
          <tr>
            <td><strong>group 分组</strong></td>
            <td><code>group "名称" {'{'} ... {'}'}</code></td>
            <td>将多个实体归入同一逻辑分组（如子系统、区域）</td>
            <td><code>group "后端" {'{'} entity svc ... {'}'}</code></td>
          </tr>
        </tbody>
      </table>

      <h2>三种箭头的语义</h2>
      <p>Plotgram 刻意只保留 3 种箭头，让 AI 和人类都能准确表达数据流含义，没有歧义。</p>
      <div className="doc-feature-grid">
        <div className="doc-feature-card">
          <div className="icon">→</div>
          <h4><code>-&gt;</code> 主动数据流/调用</h4>
          <p>实线箭头，表示主动发起的请求、调用、命令或数据写入。例如：客户端 → API 网关。</p>
        </div>
        <div className="doc-feature-card">
          <div className="icon">⇢</div>
          <h4><code>--&gt;</code> 被动响应/返回</h4>
          <p>虚线箭头，表示被动的响应、返回结果或事件通知。例如：数据库 --&gt; 服务（返回查询结果）。</p>
        </div>
        <div className="doc-feature-card">
          <div className="icon">↔</div>
          <h4><code>&lt;-&gt;</code> 双向通信</h4>
          <p>双向箭头，表示双向实时通信、WebSocket 连接或对等交互。例如：前端 &lt;-&gt; WebSocket 服务。</p>
        </div>
      </div>

      <h2>支持的实体类型</h2>
      <p>通过 <code>type</code> 属性为实体指定语义角色，Plotgram 会自动选择对应的图标、形状和配色方案。</p>
      <table>
        <thead>
          <tr>
            <th>类型</th>
            <th>图标</th>
            <th>语义说明</th>
          </tr>
        </thead>
        <tbody>
          {ENTITY_TYPES.map((et) => (
            <tr key={et.type}>
              <td><code>{et.type}</code></td>
              <td style={{ fontSize: 20 }}>{et.icon}</td>
              <td>{et.desc}</td>
            </tr>
          ))}
        </tbody>
      </table>

      <div className="callout tip">
        <div className="callout-icon">💡</div>
        <div className="callout-body">
          <strong>AI 提示</strong>
          <p>告诉 AI 你想画什么类型的图，以及实体的角色（如 "数据库"、"API 网关"、"微服务"），AI 会自动选择合适的 type 和布局方向。</p>
        </div>
      </div>

      <h2>下一步</h2>
      <p>恭喜你完成了快速上手！接下来你可以深入了解以下内容：</p>
      <div className="quick-links">
        <a href="/docs/agent-guide/" className="quick-link-card">
          <div className="ql-icon">🤖</div>
          <h4>阅读 Agent 集成指南</h4>
          <p>了解如何将 Plotgram 集成到你的 AI Agent 工作流中</p>
          <span className="ql-arrow">→</span>
        </a>
        <a href="/showcase/" className="quick-link-card">
          <div className="ql-icon">📂</div>
          <h4>浏览 70+ 示例画廊</h4>
          <p>从真实场景的示例中学习更多语法技巧</p>
          <span className="ql-arrow">→</span>
        </a>
        <a href="/docs/how-it-works/" className="quick-link-card">
          <div className="ql-icon">🔧</div>
          <h4>了解技术架构</h4>
          <p>深入解析 Plotgram 渲染引擎的设计与实现</p>
          <span className="ql-arrow">→</span>
        </a>
      </div>
    </DocPage>
  );
}
