import DocPage, { DOCS_SIDEBAR } from '../components/DocPage';
import CodeBlock from '../components/CodeBlock';

const sidebar = DOCS_SIDEBAR.map((section) => ({
  ...section,
  items: section.items.map((item) => ({
    ...item,
    active: item.label === '快速上手',
  })),
}));

const helloFlowchart = `diagram flowchart {
    title: "Hello Plotgram"
    config { direction: left-to-right }

    entity[start] start "开始"
    entity[process] step "处理"
    entity[end] end "结束"

    start -> step
    step -> end
}`;

const semanticFlowchart = `diagram flowchart {
    title: "用户认证流程"
    config { direction: top-to-bottom }

    entity[client] client "客户端"
    entity[gateway] gateway "API 网关"
    entity[service] auth "认证服务"
    entity[database] db "用户数据库"
    entity[cache] cache "Token 缓存"

    client -> gateway "HTTPS 请求"
    gateway -> auth "转发认证"
    auth -> db "查询用户"
    db --> auth "返回记录"
    auth -> cache "存储 Token"
    cache --> auth "缓存命中"
    auth --> gateway "认证结果"
    gateway --> client "响应"
}`;

const helloSequence = `diagram sequence {
    title: "API 请求响应"

    entity[boundary] client "客户端"
    entity[control] server "服务端"
    entity[database] db "数据库"

    client -> server "GET /api/user"
    server -> db "SELECT * FROM users"
    db --> server "user data"
    server --> client "200 OK { ... }"
}`;

const helloArchitecture = `diagram architecture {
    title: "三层架构"

    entity[frontend] client "客户端" { semantic: browser }
    entity[service] api "API 服务"
    entity[database] db "数据库" { semantic: postgres }

    client -> api "HTTP 请求"
    api -> db "SQL 查询"
    db --> api "查询结果"
    api --> client "JSON 响应"
}`;

const FLOWCHART_TYPES = [
  { type: 'start', desc: '流程起点' },
  { type: 'end', desc: '流程终点' },
  { type: 'process', desc: '处理步骤（默认矩形）' },
  { type: 'decision', desc: '判断/分支（菱形）' },
  { type: 'service', desc: '微服务/API 服务' },
  { type: 'database', desc: '数据库（圆柱形）' },
  { type: 'gateway', desc: 'API 网关/入口' },
  { type: 'cache', desc: '缓存层（Redis 等）' },
  { type: 'queue', desc: '消息队列' },
  { type: 'client', desc: '客户端应用' },
  { type: 'person', desc: '用户/参与者' },
  { type: 'storage', desc: '对象存储' },
  { type: 'external', desc: '外部系统' },
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
        <a href="https://github.com/plotgram/plotgram" target="_blank" rel="noopener noreferrer" className="quick-link-card">
          <div className="ql-icon">💻</div>
          <h4>CLI 命令行</h4>
          <p>本地渲染 SVG/PNG，支持批量处理和 CI 集成</p>
          <span className="ql-arrow">→</span>
        </a>
        <a href="/playground/" className="quick-link-card">
          <div className="ql-icon">🧩</div>
          <h4>WASM / HTTP API</h4>
          <p>嵌入前端或后端服务，Agent 集成首选</p>
          <span className="ql-arrow">→</span>
        </a>
      </div>

      <h2>第一步：Hello World 流程图</h2>
      <p>
        打开 <a href="/playground/">Playground</a>，在左侧编辑器中输入以下代码，右侧会实时渲染出你的第一张 Plotgram 图：
      </p>
      <CodeBlock code={helloFlowchart} title="hello.pgm" />

      <h3>逐行解读</h3>
      <ul>
        <li><code>diagram flowchart {'{'}</code> — 声明这是一张流程图，Plotgram 会自动选择最优布局算法</li>
        <li><code>title: "Hello Plotgram"</code> — 图表标题，直接写在 diagram body 中（不在 config 块内）</li>
        <li><code>config {'{'} direction: left-to-right {'}'}</code> — 配置块，设置布局方向为从左到右（默认是 top-to-bottom）</li>
        <li><code>entity[start] start "开始"</code> — 声明一个类型为 <code>start</code> 的实体，id 为 <code>start</code>，显示标签为"开始"</li>
        <li><code>start -{'>'} step</code> — 用实线箭头连接两个实体，表示主动的流程推进</li>
      </ul>

      <h2>第二步：语义类型 + 响应箭头</h2>
      <p>
        给实体指定语义类型（写在方括号中），Plotgram 会自动选择合适的图标和形状。使用 <code>--{'>'}</code> 虚线箭头表示"响应/返回"方向：
      </p>
      <CodeBlock code={semanticFlowchart} title="user-auth.pgm" />
      <div className="callout tip">
        <div className="callout-icon">💡</div>
        <div className="callout-body">
          <strong>语法要点</strong>
          <p>
            <strong>type 写在方括号里</strong>：<code>entity[service] auth "认证服务"</code>，而不是在属性块里写 <code>{'{'} type: service {'}'}</code>。这是与旧版本最大的区别。
          </p>
        </div>
      </div>

      <h2>第三步：尝试其他图表类型</h2>
      <p>Plotgram 支持 6 种图表类型，每种都有专属的默认布局和实体类型。切换 <code>diagram</code> 后面的关键字即可。</p>

      <h3>时序图（Sequence）</h3>
      <p>适合描述 API 调用、消息传递等交互时序。时序图不需要设置 direction，参与者按声明顺序排列。</p>
      <CodeBlock code={helloSequence} title="api-sequence.pgm" />
      <p>时序图支持的实体类型：<code>participant</code>（默认参与者）、<code>actor</code>（人形角色）、<code>boundary</code>（边界/入口）、<code>control</code>（控制器）、<code>database</code>（数据库）。</p>

      <h3>架构图（Architecture）</h3>
      <p>适合描述微服务架构、系统分层、组件拓扑。架构图<strong>不支持</strong> <code>direction</code> 属性，默认采用水平分层布局。</p>
      <CodeBlock code={helloArchitecture} title="three-tier.pgm" />
      <p>架构图支持的实体类型：<code>frontend</code>、<code>backend</code>、<code>service</code>、<code>database</code>、<code>gateway</code>、<code>cache</code>、<code>queue</code>、<code>storage</code>、<code>external</code>。</p>

      <h2>核心语法：只有 5 个概念</h2>
      <p>Plotgram 遵循"最少概念"原则——掌握以下 5 个核心元素，就能描述绝大多数技术图表。</p>
      <table>
        <thead>
          <tr>
            <th>元素</th>
            <th>语法</th>
            <th>说明</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <td><strong>diagram 声明</strong></td>
            <td><code>diagram &lt;类型&gt; {'{'}</code></td>
            <td>声明图表类型：flowchart / sequence / architecture / state / er / mindmap</td>
          </tr>
          <tr>
            <td><strong>title 标题</strong></td>
            <td><code>title: "&lt;标题&gt;"</code></td>
            <td>图表标题，直接写在 body 级，<strong>不</strong>放在 config 块中</td>
          </tr>
          <tr>
            <td><strong>entity 实体</strong></td>
            <td><code>entity[&lt;type&gt;] &lt;id&gt; "&lt;标签&gt;"</code></td>
            <td>定义节点。type 在方括号中（可选），id 用于后续引用，标签是显示文字</td>
          </tr>
          <tr>
            <td><strong>arrow 箭头</strong></td>
            <td><code>a -{'>'} b "标签"</code></td>
            <td>连接两个实体。仅 3 种箭头，语义固定无歧义</td>
          </tr>
          <tr>
            <td><strong>config 配置</strong></td>
            <td><code>config {'{'} ... {'}'}</code></td>
            <td>可选，集中设置 direction、theme、edge_routing、group_frame 等渲染参数</td>
          </tr>
        </tbody>
      </table>

      <h2>三种箭头，固定语义</h2>
      <p>Plotgram 刻意只保留 3 种箭头，让 AI 和人类都能准确表达数据流含义，没有歧义。这是 LLM 生成正确率高的关键设计之一。</p>
      <div className="doc-feature-grid">
        <div className="doc-feature-card">
          <div className="icon">→</div>
          <h4><code>-{'>'}</code> 主动流向</h4>
          <p>实线箭头，表示主动发起的请求、调用、命令、流程推进。例如：客户端 → API 网关。</p>
        </div>
        <div className="doc-feature-card">
          <div className="icon">⇢</div>
          <h4><code>--{'>'}</code> 被动响应</h4>
          <p>虚线箭头，表示被动的响应、返回结果、事件通知。例如：数据库 --{'>'} 服务（返回查询结果）。</p>
        </div>
        <div className="doc-feature-card">
          <div className="icon">↔</div>
          <h4><code>{'<->'}</code> 双向通信</h4>
          <p>双向箭头，表示双向实时通信、WebSocket 连接、对等交互。例如：前端 {'<->'} WebSocket 服务。</p>
        </div>
      </div>

      <h2>流程图实体类型速查</h2>
      <p>流程图是最常用的图表类型。通过 <code>entity[&lt;type&gt;]</code> 为实体指定语义角色，Plotgram 自动匹配对应的形状和图标。</p>
      <table>
        <thead>
          <tr>
            <th>type</th>
            <th>说明</th>
            <th>典型形状</th>
          </tr>
        </thead>
        <tbody>
          {FLOWCHART_TYPES.map((et) => (
            <tr key={et.type}>
              <td><code>{et.type}</code></td>
              <td>{et.desc}</td>
              <td style={{ color: 'var(--text-muted)', fontSize: 13 }}>自动选择</td>
            </tr>
          ))}
        </tbody>
      </table>

      <div className="callout tip">
        <div className="callout-icon">🤖</div>
        <div className="callout-body">
          <strong>AI 友好提示</strong>
          <p>
            告诉 AI 你想画什么类型的图，以及实体的角色（如 "数据库"、"API 网关"、"微服务"），AI 会自动选择合适的 type 和布局方向。不需要记忆 type 枚举值，用自然语言描述即可。
          </p>
        </div>
      </div>

      <h2>常见误区</h2>
      <ul>
        <li>
          <strong>type 写在属性块里</strong>：旧语法 <code>entity step "处理" {'{'} type: service {'}'}</code> 已改为方括号语法 <code>entity[service] step "处理"</code>
        </li>
        <li>
          <strong>title 放在 config 里</strong>：<code>title</code> 是 body 级属性，必须写在 <code>config {'{'} ... {'}'}</code> 外面
        </li>
        <li>
          <strong>架构图设置 direction</strong>：architecture 类型不支持 <code>direction</code> 属性，删掉即可
        </li>
        <li>
          <strong>使用了不支持的 type</strong>：不同图表类型支持的 type 不同，例如 sequence 图不能用 <code>service</code>，要用 <code>participant</code>
        </li>
        <li>
          <strong>边样式引用错误</strong>：声明 <code>edge_style error {'{'} ... {'}'}</code> 后，引用时用 <code>{'{'} line_style: error {'}'}</code>，不是 <code>edge_style: error</code>
        </li>
      </ul>

      <h2>下一步</h2>
      <p>恭喜你完成了快速上手！接下来你可以：</p>
      <div className="quick-links">
        <a href="/playground/" className="quick-link-card">
          <div className="ql-icon">🚀</div>
          <h4>打开 Playground 动手尝试</h4>
          <p>在浏览器中实时编写 Plotgram 代码</p>
          <span className="ql-arrow">→</span>
        </a>
        <a href="/showcase/" className="quick-link-card">
          <div className="ql-icon">📂</div>
          <h4>浏览示例画廊</h4>
          <p>70+ 真实场景示例，涵盖所有 6 种图表类型</p>
          <span className="ql-arrow">→</span>
        </a>
        <a href="/docs/agent-guide/" className="quick-link-card">
          <div className="ql-icon">🤖</div>
          <h4>Agent 集成指南</h4>
          <p>了解如何将 Plotgram 集成到 AI Agent 工作流中</p>
          <span className="ql-arrow">→</span>
        </a>
      </div>
    </DocPage>
  );
}
