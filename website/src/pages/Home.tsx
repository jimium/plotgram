import HeroPlayground from '../components/HeroPlayground';
import ScrollNav from '../components/ScrollNav';

const DIAGRAM_TYPES = [
  { icon: '🔀', name: '流程图 Flowchart', desc: '业务流程、审批流、CI/CD', status: 'stable' },
  { icon: '📊', name: '时序图 Sequence', desc: 'API 调用、微服务交互', status: 'stable' },
  { icon: '🏗️', name: '架构图 Architecture', desc: '云原生、微服务、系统拓扑', status: 'stable' },
  { icon: '🔄', name: '状态机 State', desc: '订单生命周期、状态流转', status: 'beta' },
  { icon: '🗃️', name: 'ER 图 ER Diagram', desc: '数据库设计、数据建模', status: 'beta' },
  { icon: '🧠', name: '思维导图 Mindmap', desc: '知识梳理、产品路线图', status: 'beta' },
];

const EXPORT_FORMATS = [
  { icon: '📄', name: 'SVG', desc: '矢量图，无损缩放' },
  { icon: '🖼️', name: 'PNG', desc: '透明底位图' },
  { icon: '🌐', name: 'WebP', desc: '高效压缩位图' },
  { icon: '📐', name: 'Draw.io', desc: '可继续编辑' },
  { icon: '🔤', name: 'ASCII', desc: '终端文本图' },
  { icon: '📋', name: 'JSON', desc: 'AST 可编程' },
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

const LAYOUT_ALGOS = [
  { name: 'Sugiyama-v2', desc: '增强分层布局', tag: '流程图/ER图', color: 'purple' },
  { name: 'Architecture', desc: '双层分组布局', tag: '架构图', color: 'cyan' },
  { name: 'Mindmap', desc: '中心辐射布局', tag: '思维导图', color: 'purple' },
  { name: 'Force-Directed', desc: '力导向布局', tag: '拓扑图', color: 'cyan' },
  { name: 'Circular', desc: '自适应环形布局', tag: '状态机', color: 'purple' },
  { name: 'Sequence', desc: '生命线专用布局', tag: '时序图', color: 'cyan' },
];

const ROUTING_PIPELINE = [
  { step: '01', title: '通道规划', desc: '基于节点 rank 与分组边界，自动规划正交通道与走廊，边沿分组边缘绕行' },
  { step: '02', title: '边聚合 & 锚点共享', desc: '同方向边共享槽位锚点、合并主干，减少视觉杂乱（Edge Bundling）' },
  { step: '03', title: '冲突重路由 + 车道分配', desc: '3 轮渐进式避让重路由，交叉轴偏移分配车道，保证边不重叠、不穿节点' },
];

const COMPARISON = [
  { label: '语法变体与隐式规则', legacy: '多种箭头风格、隐式写法多', legacyMark: 'cross' as const, plotgram: '固定语法——3 种箭头、显式结构', plotgramMark: 'check' as const },
  { label: '自动布局', legacy: '需要手动调坐标/hint', legacyMark: 'partial' as const, plotgram: '语义优先，引擎全自动布局', plotgramMark: 'check' as const },
  { label: '错误反馈', legacy: '静默失败或模糊文本错误', legacyMark: 'cross' as const, plotgram: '结构化 JSON 诊断含修复建议', plotgramMark: 'check' as const },
  { label: '可编程性', legacy: '文本是唯一产物', legacyMark: 'cross' as const, plotgram: 'AST 导出、语义 Diff & Patch', plotgramMark: 'check' as const },
  { label: '多端交付', legacy: '通常只有 CLI/Web', legacyMark: 'partial' as const, plotgram: 'CLI / HTTP API / WASM 同源核心', plotgramMark: 'check' as const },
];

function Mark({ type }: { type: 'check' | 'cross' | 'partial' }) {
  if (type === 'check') return <span className="comparison-check">✓</span>;
  if (type === 'cross') return <span className="comparison-cross">✗</span>;
  return <span className="comparison-partial">◐</span>;
}

export default function Home() {
  return (
    <div>
      <ScrollNav />
      {/* HERO */}
      <section className="hero" id="hero">
        <div className="container">
          <h1>
            让 <span className="gradient-text">Agent</span> 会画图<br />
            一图胜千言，被 AI 放大
          </h1>
          <p className="hero-subtitle">
            Plotgram 是为 AI 生成而设计的图表 DSL——Agent 理解语义、操作 AST，
            按你的需求生成和修改图表，改一行不用重画整张。80+ 真实示例，对话即可出图。
          </p>
          <div className="hero-actions">
            <a href="/agent/" className="btn btn-primary" target="_blank" rel="noopener noreferrer">
              去 Agent Demo 对话
              <span className="hero-cta-arrow">→</span>
            </a>
            <a href="/showcase/" className="btn btn-showcase" target="_blank" rel="noopener noreferrer">
              浏览 80+ 示例
              <span className="hero-cta-arrow">→</span>
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

      {/* OVERVIEW */}
      <section className="judges" id="overview">
        <div className="container">
          <div className="judges-card">
            <div className="judges-header">
              <span className="judges-badge">项目概览</span>
              <h2>为什么 Plotgram 值得关注</h2>
              <p>30 秒理解项目核心价值与差异化优势</p>
            </div>
            <div className="judges-grid">
              {/* Column 1: Core Innovation */}
              <div className="judges-col">
                <div className="judges-col-icon">💡</div>
                <h3>核心创新</h3>
                <p className="judges-col-desc">
                  <strong>AI 原生的图表 DSL</strong>——语法为 LLM 理解语义、操作 AST 而设计。
                  不是"让 AI 写 Mermaid"，而是"创造一种 AI 天生就能写对的图表语言"。
                </p>
                <ul className="judges-tags">
                  <li>结构化 AST，可序列化为 JSON</li>
                  <li>语义 Diff &amp; Patch，增量修改不重生成</li>
                  <li>结构化错误含修复建议，AI 一次自修正</li>
                  <li>支持通过 Skill + MCP 深度集成 AI 编程工具</li>
                </ul>
              </div>

              {/* Column 2: Quantitative Data */}
              <div className="judges-col">
                <div className="judges-col-icon">📊</div>
                <h3>量化数据</h3>
                <div className="judges-stats">
                  <div className="judges-stat">
                    <span className="judges-stat-num">50K+</span>
                    <span className="judges-stat-label">行 Rust 源码</span>
                  </div>
                  <div className="judges-stat">
                    <span className="judges-stat-num">952</span>
                    <span className="judges-stat-label">个测试用例</span>
                  </div>
                  <div className="judges-stat">
                    <span className="judges-stat-num">83</span>
                    <span className="judges-stat-label">个真实场景示例</span>
                  </div>
                  <div className="judges-stat">
                    <span className="judges-stat-num">45 天</span>
                    <span className="judges-stat-label">从 0 到 50K 行 Rust</span>
                  </div>
                </div>
              </div>

              {/* Column 3: Comparison */}
              <div className="judges-col">
                <div className="judges-col-icon">⚡</div>
                <h3>对比传统方案</h3>
                <div className="judges-compare">
                  <div className="judges-compare-row">
                    <span className="judges-compare-label">语法设计</span>
                    <span className="judges-compare-bad">Mermaid 多种箭头变体、隐式规则</span>
                    <span className="judges-compare-good">Plotgram 3 种箭头、显式声明</span>
                  </div>
                  <div className="judges-compare-row">
                    <span className="judges-compare-label">错误反馈</span>
                    <span className="judges-compare-bad">PlantUML 静默失败或模糊文本</span>
                    <span className="judges-compare-good">结构化 JSON 含行列+修复建议</span>
                  </div>
                  <div className="judges-compare-row">
                    <span className="judges-compare-label">可编程性</span>
                    <span className="judges-compare-bad">文本是唯一产物</span>
                    <span className="judges-compare-good">AST 导出、Diff &amp; Patch</span>
                  </div>
                  <div className="judges-compare-row">
                    <span className="judges-compare-label">多端交付</span>
                    <span className="judges-compare-bad">通常只有 CLI/Web</span>
                    <span className="judges-compare-good">CLI / API / WASM 同源</span>
                  </div>
                </div>
              </div>
            </div>
          </div>
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

      {/* ALGORITHMS */}
      <section className="algorithms" id="algorithms">
        <div className="container">
          <div className="section-header">
            <div className="section-label">Core Engine</div>
            <h2>自研布局与路由引擎</h2>
            <p>50K+ 行 Rust 实现的核心算法——不是包装开源库，而是从几何底层自研</p>
          </div>
          <div className="algo-grid">
            <div className="algo-card">
              <div className="algo-card-header">
                <div className="algo-card-icon purple">📐</div>
                <div>
                  <h3>7 种节点布局算法</h3>
                  <p>根据图表类型自动切换，引擎选择最优策略</p>
                </div>
              </div>
              <div className="algo-list">
                {LAYOUT_ALGOS.map((a) => (
                  <div className="algo-item" key={a.name}>
                    <span className={`algo-dot ${a.color}`} />
                    <div className="algo-item-body">
                      <div className="algo-item-name">{a.name}</div>
                      <div className="algo-item-desc">{a.desc}</div>
                    </div>
                    <span className="algo-item-tag">{a.tag}</span>
                  </div>
                ))}
              </div>
            </div>

            <div className="algo-card">
              <div className="algo-card-header">
                <div className="algo-card-icon cyan">✏️</div>
                <div>
                  <h3>正交边路由三层管线</h3>
                  <p>复杂架构图也能清晰不交叉、不穿节点</p>
                </div>
              </div>
              <div className="routing-pipeline">
                {ROUTING_PIPELINE.map((p, i) => (
                  <div className="pipeline-item" key={p.step}>
                    <div className="pipeline-step">
                      <div className="pipeline-step-num">{p.step}</div>
                      <div className="pipeline-step-body">
                        <div className="pipeline-step-title">{p.title}</div>
                        <div className="pipeline-step-desc">{p.desc}</div>
                      </div>
                    </div>
                    {i < ROUTING_PIPELINE.length - 1 && <div className="pipeline-arrow">↓</div>}
                  </div>
                ))}
              </div>
              <div className="algo-highlight">
                <strong>+</strong> 还支持 Bezier 曲线、Spline 样条、Organic 自然肘形 S 曲线、Circular 弧形等路由风格
              </div>
            </div>
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
              </a>
            ))}
          </div>

          <div className="export-section">
            <div className="section-header">
              <div className="section-label">Export</div>
              <h2>多格式导出，无缝融入工作流</h2>
              <p>CLI / API / WASM 统一输出，满足文档、演示、二次编辑、程序化处理需求</p>
            </div>
            <div className="export-grid">
              {EXPORT_FORMATS.map((f) => (
                <div className="export-card" key={f.name}>
                  <div className="export-icon">{f.icon}</div>
                  <div className="export-name">{f.name}</div>
                  <div className="export-desc">{f.desc}</div>
                </div>
              ))}
            </div>
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

    </div>
  );
}
