import HeroPlayground from '../components/HeroPlayground';

const DIAGRAM_TYPES = [
  { icon: '🔀', name: '流程图 Flowchart', desc: '业务流程、审批流、CI/CD', status: 'stable' },
  { icon: '📊', name: '时序图 Sequence', desc: 'API 调用、微服务交互', status: 'stable' },
  { icon: '🏗️', name: '架构图 Architecture', desc: '云原生、微服务、系统拓扑', status: 'stable' },
  { icon: '🔄', name: '状态机 State', desc: '订单生命周期、状态流转', status: 'beta' },
  { icon: '🗃️', name: 'ER 图 ER Diagram', desc: '数据库设计、数据建模', status: 'beta' },
  { icon: '🧠', name: '思维导图 Mindmap', desc: '知识梳理、产品路线图', status: 'beta' },
];

const FEATURES = [
  {
    icon: '🤖',
    title: 'AI 原生语法设计',
    desc: '箭头仅 3 种、语义用 type 表达、没有隐式规则——LLM 一次写对，不用调参猜语法。',
  },
  {
    icon: '⚠️',
    title: '结构化错误自修复',
    desc: '错误返回 JSON：错误码、行列位置、上下文、修复建议。Agent 一次重试即可自我修正，告别渲染空白。',
  },
  {
    icon: '🔧',
    title: 'AST 一等公民',
    desc: 'AST 可序列化为 JSON、支持语义级 Diff 与 Patch。Agent 可以增量修改，不需要重生成整张图。',
  },
  {
    icon: '📐',
    title: '7 种自动布局算法',
    desc: 'Sugiyama-v2 分层、力导向、圆形、思维导图、时序、架构图双层布局——引擎自动选择最优布局。',
  },
  {
    icon: '✏️',
    title: '正交路由 + 边聚合',
    desc: '三层正交边路由管线：通道规划 → 边聚合（Edge Bundling）→ 冲突重路由，复杂图也清晰不交叉。',
  },
  {
    icon: '🎨',
    title: '语义图标 + 多套主题',
    desc: '50+ 内置语义图标（数据库、服务、K8s 资源等），7 套精美主题一键切换，技术图也能颜值在线。',
  },
];

const COMPARISON = [
  { label: '语法变体与隐式规则', legacy: '多种箭头风格、隐式写法多', legacyMark: 'cross' as const, plotgram: '固定语法——3 种箭头、显式结构', plotgramMark: 'check' as const },
  { label: '自动布局', legacy: '需要手动调坐标/hint', legacyMark: 'partial' as const, plotgram: '语义优先，引擎全自动布局', plotgramMark: 'check' as const },
  { label: '错误反馈', legacy: '静默失败或模糊文本错误', legacyMark: 'cross' as const, plotgram: '结构化 JSON 诊断含修复建议', plotgramMark: 'check' as const },
  { label: '可编程性', legacy: '文本是唯一产物', legacyMark: 'cross' as const, plotgram: 'AST 导出、语义 Diff & Patch', plotgramMark: 'check' as const },
  { label: '多端交付', legacy: '通常只有 CLI/Web', legacyMark: 'partial' as const, plotgram: 'CLI / HTTP API / WASM 同源核心', plotgramMark: 'check' as const },
];

const STATS = [
  { number: '70+', label: '真实场景示例' },
  { number: '6', label: '图表类型 · 一套语法' },
  { number: '3', label: '端同源交付 CLI / API / WASM' },
  { number: '0', label: '手写坐标 · 全自动布局' },
];

function Mark({ type }: { type: 'check' | 'cross' | 'partial' }) {
  if (type === 'check') return <span className="comparison-check">✓</span>;
  if (type === 'cross') return <span className="comparison-cross">✗</span>;
  return <span className="comparison-partial">◐</span>;
}

export default function Home() {
  return (
    <div>
      {/* HERO */}
      <section className="hero">
        <div className="container">
          <a
            className="hero-badge"
            href="https://www.trae.cn/ai-creativity?utm_source=community"
            target="_blank"
            rel="noopener noreferrer"
          >
            <span className="hero-badge-dot" />
            TRAE AI 编程大赛参赛作品
          </a>
          <h1>
            让 <span className="gradient-text">Agent</span> 会画图<br />
            一图胜千言，被 AI 放大
          </h1>
          <p className="hero-subtitle">
            Plotgram 是为 AI 生成而设计的图表 DSL——Agent 理解语义、操作 AST，
            按你的需求生成和修改图表，改一行不用重画整张。70+ 真实示例，对话即可出图。
          </p>
          <div className="hero-actions">
            <a href="/agent/" className="btn btn-primary" target="_blank" rel="noopener noreferrer">
              去 Agent Demo 对话
              <span className="hero-cta-arrow">→</span>
            </a>
            <a href="/showcase/" className="btn btn-secondary" target="_blank" rel="noopener noreferrer">
              浏览 70+ 示例
            </a>
            <a href="/playground/" className="btn btn-agent" target="_blank" rel="noopener noreferrer">
              打开 Playground
            </a>
            <a href="/docs/getting-started/" className="btn btn-ghost">
              5 分钟快速上手
            </a>
          </div>

          <HeroPlayground />
        </div>
      </section>

      {/* FEATURES */}
      <section className="features" id="features">
        <div className="container">
          <div className="section-header">
            <div className="section-label">Core Features</div>
            <h2>Agent 画图，为什么需要专门的 DSL</h2>
            <p>每一处设计，都为了让 AI 一次生成对的图、增量改对已有的图</p>
          </div>
          <div className="features-grid">
            {FEATURES.map((f) => (
              <div className="feature-card" key={f.title}>
                <div className="feature-icon">{f.icon}</div>
                <h3>{f.title}</h3>
                <p>{f.desc}</p>
              </div>
            ))}
          </div>
        </div>
      </section>

      {/* DIAGRAM TYPES */}
      <section className="diagram-types" id="diagram-types">
        <div className="container">
          <div className="section-header">
            <div className="section-label">Diagram Types</div>
            <h2>6 种图表，覆盖主流场景</h2>
            <p>引擎根据图表类型自动选择最优布局算法</p>
          </div>
          <div className="types-grid">
            {DIAGRAM_TYPES.map((t) => (
              <a href="/showcase/" className="type-card" key={t.name} target="_blank" rel="noopener noreferrer">
                <div className="type-icon">{t.icon}</div>
                <h3>{t.name}</h3>
                <p>{t.desc}</p>
                <span className={`type-status ${t.status}`}>
                  {t.status === 'stable' ? 'Stable' : 'Beta'}
                </span>
              </a>
            ))}
          </div>
        </div>
      </section>

      {/* COMPARISON */}
      <section className="comparison" id="comparison">
        <div className="container">
          <div className="section-header">
            <div className="section-label">Why Plotgram</div>
            <h2>为 AI 生成而设计，从一开始</h2>
            <p>传统工具为人类手写优化，Plotgram 为 Agent 生成优化——语境不同，设计不同</p>
          </div>
          <div className="comparison-table">
            <div className="comparison-row header">
              <div className="comparison-cell">维度</div>
              <div className="comparison-cell" style={{ color: '#94A3B8' }}>传统工具 (Mermaid/PlantUML)</div>
              <div
                className="comparison-cell"
                style={{
                  background: 'linear-gradient(135deg, #7C3AED, #06B6D4)',
                  WebkitBackgroundClip: 'text',
                  WebkitTextFillColor: 'transparent',
                  backgroundClip: 'text',
                  fontWeight: 700,
                }}
              >
                Plotgram
              </div>
            </div>
            {COMPARISON.map((row) => (
              <div className="comparison-row" key={row.label}>
                <div className="comparison-cell" data-label="维度">{row.label}</div>
                <div className="comparison-cell legacy" data-label="传统工具">
                  <Mark type={row.legacyMark} />
                  {row.legacy}
                </div>
                <div className="comparison-cell plotgram" data-label="Plotgram">
                  <Mark type={row.plotgramMark} />
                  {row.plotgram}
                </div>
              </div>
            ))}
          </div>
        </div>
      </section>

      {/* STATS */}
      <section className="stats">
        <div className="container">
          <div className="stats-grid">
            {STATS.map((s) => (
              <div key={s.label}>
                <div className="stat-number">{s.number}</div>
                <div className="stat-label">{s.label}</div>
              </div>
            ))}
          </div>
        </div>
      </section>

    </div>
  );
}
