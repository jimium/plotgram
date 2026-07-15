import { useState } from 'react';
import HeroPlayground from '../components/HeroPlayground';

interface RoadmapItem {
  phase: string;
  icon: string;
  title: string;
  status: string;
  desc: string;
  bullets: string[];
  demo?: string | { src: string; label: string }[];
  demoType?: 'svg' | 'animation';
}

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

const ROADMAP_ITEMS: RoadmapItem[] = [
  {
    phase: '近期',
    icon: '🎬',
    title: '动画效果',
    status: 'planning',
    desc: '为 SVG 输出加入动画层——节点入场、路径绘制、状态流转。让 Agent 生成的图不只是静态结果，而是可以"动起来"讲故事。',
    bullets: [
      '节点入场与退出动画（fade / slide / scale）',
      '路径绘制动画（stroke-dashoffset）展示数据流向',
      '状态机状态流转动画',
      '渲染引擎内置动画属性 vs 前端独立动画层的取舍',
    ],
    demo: '/assets/animation-demo.svg',
    demoType: 'animation',
  },
  {
    phase: '近期',
    icon: '🏛️',
    title: 'C4 架构图',
    status: 'planning',
    desc: '支持 C4 模型四层语义（Context / Container / Component / Code），覆盖企业架构文档场景。需要新的 entity 类型与 diagram 变体。',
    bullets: [
      'Context / Container / Component / Code 四层图表类型',
      'C4 语义实体类型（Person / System / Container / Component）',
      '层级间导航与下钻',
      '企业架构文档场景示例',
    ],
  },
  {
    phase: '中期',
    icon: '🔌',
    title: 'MCP Server',
    status: 'planned',
    desc: '提供 plotgram-mcp crate，让 Cursor / Claude Code / Copilot 等 AI 编程工具的 Agent 能直接生成、验证、修改 Plotgram DSL。',
    bullets: [
      '暴露 render / validate / diff / patch 能力为 MCP 工具',
      'AI 编程工具是 Plotgram 的天然分发渠道',
      '与现有 Agent Demo 形成互补：Demo 面向终端用户，MCP 面向开发者工具链',
    ],
  },
  {
    phase: '中期',
    icon: '📡',
    title: '实时数据源集成',
    status: 'planned',
    desc: '读取 Kubernetes、Elasticsearch 等 API 的实时状态，自动绘制集群拓扑图、服务健康状态图、索引分布图。让基础设施可视化从"手工画图"变成"实时观测"。',
    bullets: [
      'K8s API 集成：自动绘制集群拓扑、Pod 分布、Service 依赖图',
      'Elasticsearch API 集成：索引分片图、节点状态图',
      'Prometheus / Grafana 数据源：指标驱动的动态图表',
      '声明式数据源配置，Agent 自动发现与绘制',
    ],
    demo: [
      { src: '/assets/es-cluster.svg', label: '集群拓扑' },
      { src: '/assets/es-index-shards.svg', label: '索引分片' },
      { src: '/assets/es-search-flow.svg', label: '搜索流程' },
      { src: '/assets/es-node-health.svg', label: '节点健康' },
    ],
    demoType: 'svg',
  },
  {
    phase: '中期',
    icon: '🧩',
    title: 'VS Code 插件',
    status: 'planned',
    desc: '语法高亮 + 实时预览 + 错误诊断。基于 WASM 在插件内直接渲染，无需外部服务。',
    bullets: [
      '.pgm 文件语法高亮与自动补全',
      '侧边栏实时 SVG 预览',
      '结构化错误诊断（LSP 风格）',
      'Snippet 模板快速插入',
    ],
  },
  {
    phase: '中期',
    icon: '🔄',
    title: 'GitHub Action',
    status: 'planned',
    desc: '提交 .pgm 文件到仓库时，CI 自动渲染 SVG/PNG 并附加到 PR comment。最低成本的开发者生态入口。',
    bullets: [
      '.pgm 文件变更自动渲染为 SVG/PNG',
      'PR comment 展示渲染结果与语义 diff',
      '与现有 AST Diff & Patch 能力结合，做"图表即代码审查"',
    ],
  },
  {
    phase: '中期',
    icon: '🚀',
    title: 'Plotgram Studio',
    status: 'planned',
    desc: '计划 10 月发布在线 Studio 平台，提供可视化图表编辑器、项目管理、团队协作能力，进行商业化探索。',
    bullets: [
      '可视化图表编辑器：拖拽 + DSL 双模式编辑',
      '项目管理：多图组织、版本历史、团队分享',
      '模板市场：社区贡献的场景模板一键复用',
      '商业化探索：免费层 + Pro 订阅，面向企业与个人开发者',
    ],
  },
  {
    phase: '中期',
    icon: '📂',
    title: 'GitHub 开源计划',
    status: 'planned',
    desc: '核心引擎与 DSL 规范开源到 GitHub，构建开发者社区。核心 Rust 引擎、WASM 绑定、官方工具链以开源形式发布，商业化能力通过 Studio 与企业服务提供。',
    bullets: [
      'plotgram-core 核心引擎开源（Rust）',
      'DSL 语法规范与 AST 定义公开',
      'plotgram-wasm 绑定与 Playground 源码开源',
      '社区贡献指南与第三方插件生态',
    ],
  },
  {
    phase: '远期',
    icon: '🌐',
    title: '平台集成',
    status: 'exploring',
    desc: '争取向 Trae、Cursor 等 AI 编程工具深度集成，同时向 Claw 等 AI Agent 生态输出 Plotgram 的图表能力。',
    bullets: [
      'Trae 集成：通过 Skill / MCP 原生支持 Plotgram 图表生成',
      'Cursor / Copilot 集成：在 AI 对话中直接生成和预览图表',
      'Claw 等 Agent 生态：作为可视化输出模块嵌入第三方 Agent',
      '````plotgram 代码块在文档/静态站点中渲染',
    ],
  },
  {
    phase: '远期',
    icon: '🏢',
    title: '2B 私有化部署',
    status: 'exploring',
    desc: '探索向企业提供私有化部署版本的文本画图引擎，满足金融、政务等合规敏感行业对数据不出域的需求。',
    bullets: [
      '私有化部署套件：Docker 镜像 + 离线文档，一键部署',
      '企业级 SLA 保障：高可用架构、性能调优、专属技术支持',
      '合规适配：数据不出域，适配信创环境（麒麟、鲲鹏等）',
      '商业化探索：按节点/实例数的企业订阅授权模式',
    ],
  },
];

const STATUS_LABEL: Record<string, string> = {
  planning: '规划中',
  planned: '已排期',
  exploring: '探索中',
};

function RoadmapItemCard({ item, activeIndex, onTabChange, demoKey, onReplay }: {
  item: RoadmapItem;
  activeIndex: number;
  onTabChange: (index: number) => void;
  demoKey: number;
  onReplay: () => void;
}) {
  return (
    <div className="roadmap-item">
      <div className="roadmap-item-header">
        <span className="roadmap-item-icon">{item.icon}</span>
        <h2>{item.title}</h2>
        <span className={`roadmap-status roadmap-status-${item.status}`}>
          {item.phase} · {STATUS_LABEL[item.status]}
        </span>
      </div>
      <p>{item.desc}</p>
      <ul>
        {item.bullets.map((b) => (
          <li key={b}>{b}</li>
        ))}
      </ul>
      {item.demo && (
        <div className="roadmap-demo">
          <div className="roadmap-demo-header">
            {Array.isArray(item.demo) ? (
              <div className="roadmap-demo-tabs">
                {item.demo.map((demo, index) => (
                  <button
                    key={demo.label}
                    className={`roadmap-demo-tab ${index === activeIndex ? 'active' : ''}`}
                    onClick={() => onTabChange(index)}
                  >
                    {demo.label}
                  </button>
                ))}
              </div>
            ) : (
              <span className="roadmap-demo-label">示例预览</span>
            )}
            {!Array.isArray(item.demo) && item.demoType === 'animation' && (
              <button
                className="roadmap-demo-replay"
                onClick={onReplay}
              >
                ↻ 重播
              </button>
            )}
          </div>
          {Array.isArray(item.demo) ? (
            <img
              src={item.demo[activeIndex].src}
              className="roadmap-demo-svg"
              alt={item.demo[activeIndex].label}
            />
          ) : item.demoType === 'animation' ? (
            <iframe
              key={demoKey}
              src={item.demo}
              className="roadmap-demo-iframe"
              title="动画效果演示"
            />
          ) : (
            <img
              src={item.demo}
              className="roadmap-demo-svg"
              alt="示例图"
            />
          )}
        </div>
      )}
    </div>
  );
}

function Mark({ type }: { type: 'check' | 'cross' | 'partial' }) {
  if (type === 'check') return <span className="comparison-check">✓</span>;
  if (type === 'cross') return <span className="comparison-cross">✗</span>;
  return <span className="comparison-partial">◐</span>;
}

export default function Home() {
  const [demoKey, setDemoKey] = useState(0);
  const [activeDemoIndex, setActiveDemoIndex] = useState<Record<string, number>>({});
  const getActiveIndex = (title: string) => activeDemoIndex[title] || 0;

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

      {/* FOR JUDGES */}
      <section className="judges">
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
                  <li>计划通过 Skill + MCP 深度集成 Trae 等 Agent</li>
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

      {/* ROADMAP */}
      <section className="roadmap" id="roadmap">
        <div className="container">
          <div className="section-header">
            <div className="section-label">Roadmap</div>
            <h2>演进路线图</h2>
            <p>Plotgram 正在演进的方向——从动画效果到 C4 架构图，从开发者工具链到平台生态。所有方向都围绕一个核心目标：<strong>让 Agent 更会画图，让一图胜千言的能力被 AI 放大</strong>。</p>
          </div>

          {ROADMAP_ITEMS.map((item) => (
            <RoadmapItemCard
              key={item.title}
              item={item}
              activeIndex={getActiveIndex(item.title)}
              onTabChange={(idx) => setActiveDemoIndex(prev => ({ ...prev, [item.title]: idx }))}
              demoKey={demoKey}
              onReplay={() => setDemoKey((k) => k + 1)}
            />
          ))}

          <div className="callout tip" style={{ marginTop: 32 }}>
            <div className="callout-icon">💡</div>
            <div className="callout-body">
              <p><strong>优先级原则：</strong>近期聚焦"让图更好看、更会讲故事"（动画）和"覆盖更多企业场景"（C4）；中期补齐开发者工具链（MCP / VS Code / GitHub Action），让 Plotgram 进入日常工作流；远期做平台集成，扩大覆盖面。</p>
            </div>
          </div>
        </div>
      </section>

    </div>
  );
}
