import DocPage, { DOCS_SIDEBAR } from '../components/DocPage';

const sidebar = JSON.parse(JSON.stringify(DOCS_SIDEBAR)).map(
  (section: { label: string; items: { to: string; label: string; active?: boolean }[] }) => ({
    ...section,
    items: section.items.map((item) => ({
      ...item,
      active: item.label === 'TRAE 开发实践',
    })),
  })
);

const practices = [
  {
    title: '不要上来就写代码',
    desc: '遇到设计决策时，先让AI提出多个方案，再让AI分别评价各方案的优缺点。人做最终决策，但AI的「发散+收敛」能力极大扩展了思路。',
  },
  {
    title: '需求不清时让AI问你',
    desc: '如果需求模糊，不要让AI猜。让AI列出「还需要明确的关键决策点」，人一一回答后再开始实现。避免走弯路。',
  },
  {
    title: '感觉好用≠真的好用',
    desc: '每实现一个算法版本，建立量化指标（交叉数、ink用量、边长方差），让优化有客观依据。视觉检查+量化指标双保险。',
  },
  {
    title: '分层协作，不把整个算法丢给AI',
    desc: '把复杂算法分解为高层架构（人定）和具体实现（AI做）。人设计算法的「骨架」，AI填充「肌肉」——实现细节、边界case处理、测试用例。',
  },
  {
    title: '不要期待一次完美',
    desc: '第一版实现后，人通过测试和视觉检查找出问题，然后和AI一起分类问题、讨论改进方案，第二轮迭代后效果通常大幅提升。',
  },
  {
    title: 'EBNF/类型签名/测试都是Contract',
    desc: '在让AI写代码前，先定义好输入输出的类型签名、语法规范、测试预期。Contract越清晰，AI输出质量越高。',
  },
  {
    title: '每次一个明确的小目标',
    desc: '不要让AI「实现整个布局算法」，而是拆成「实现分层分配」「实现同层排序」「实现坐标计算」等小任务，每步验证后再继续。',
  },
];

export default function TraeStory() {
  return (
    <DocPage
      title="💪 用 TRAE 构建 Plotgram：开发实践"
      description="一个复杂算法项目的 AI 协作开发方法论——人做树的主干，AI 做叶子节点。"
      sidebar={sidebar}
    >
      <div className="callout info">
        <div className="callout-icon">ℹ️</div>
        <div className="callout-body">
          <p>Plotgram 是一个包含 30,000+ 行 Rust 核心代码、959 个测试用例、7 种布局算法和 4 种边路由策略的项目。核心引擎约 70% 的实现代码由 TRAE 生成，布局算法迭代速度比纯人工快 3-4 倍。本文分享我们的协作开发方法论。</p>
        </div>
      </div>

      <h2>协作模式：人定义架构，AI实现细节</h2>
      <p>不同于常见的「让 AI 从零写一个完整项目」模式，我们采用分层协作策略：</p>
      <div className="doc-feature-grid">
        <div className="doc-feature-card">
          <div className="icon">👤</div>
          <h4>人类负责</h4>
          <p>产品方向定义、算法选型决策、架构设计、关键API设计、质量验收标准制定</p>
        </div>
        <div className="doc-feature-card">
          <div className="icon">🤖</div>
          <h4>AI 负责</h4>
          <p>Parser实现、数据结构编码、具体算法实现、测试用例编写、代码重构与优化、Bug修复</p>
        </div>
      </div>

      <h2>开发全流程：五个关键阶段</h2>
      <div className="timeline">
        <div className="timeline-item">
          <h3>阶段一：从概念到 DSL 设计（Day 1-2）</h3>
          <div className="timeline-meta">Session: 概念探索与语法设计</div>
          <p>最初只明确了一个方向：做一个AI友好的图表语言。没有直接让AI写代码，而是先让TRAE帮助我们完成需求分析和方案对比。</p>
          <ul>
            <li>先用自然语言描述目标，让TRAE列出3种可能的语法方案</li>
            <li>再让TRAE逐一评估每个方案的优缺点（LLM生成友好度、可读性、扩展性）</li>
            <li>人选定方案后，让TRAE写出EBNF语法规范作为后续实现的contract</li>
          </ul>
        </div>

        <div className="timeline-item">
          <h3>阶段二：Parser 与基础框架搭建（Day 3-5）</h3>
          <div className="timeline-meta">Session: 基础架构搭建</div>
          <p>语法规范确定后，让TRAE基于EBNF实现手写递归下降Parser，同时搭建Rust项目结构和WASM编译目标。</p>
          <ul>
            <li>先定义AST数据结构（人审稿确认）</li>
            <li>让TRAE按优先级逐个实现语法规则的解析</li>
            <li>每完成一个语法规则，要求AI写对应的单元测试</li>
          </ul>
        </div>

        <div className="timeline-item">
          <h3>阶段三：布局算法实现与迭代（Day 6-15）</h3>
          <div className="timeline-meta">Session: 核心算法开发（最长阶段）</div>
          <p>布局算法是项目最复杂的部分，也是最需要「人机协作」的阶段。我们采用了「两轮复盘法」。</p>
          <ul>
            <li>人确定算法选型（如Sugiyama分层布局），TRAE调研具体实现细节</li>
            <li>第一版实现后，人通过视觉检查发现问题（交叉过多、边混乱等）</li>
            <li>第二轮：让TRAE对问题进行分类并提出改进方案，人选择方案后让AI实现</li>
            <li>重复上述过程直到效果达标，关键是始终有量化的benchmark衡量改进</li>
          </ul>
        </div>

        <div className="timeline-item">
          <h3>阶段四：边路由与细节优化（Day 16-22）</h3>
          <div className="timeline-meta">Session: 质量打磨</div>
          <p>基本布局可行后，开始处理边路由、Edge Bundling、像素量化等细节优化。这一阶段大量使用TRAE做小步快跑式的重构。</p>
          <ul>
            <li>每次给TRAE一个明确的小目标（如「实现正交边路由通道规划」）</li>
            <li>完成后立即通过snapshot测试验证效果</li>
            <li>发现bug时先让TRAE自己诊断，给出原因分析再修复</li>
          </ul>
        </div>

        <div className="timeline-item">
          <h3>阶段五：前端Playground与部署（Day 23-25）</h3>
          <div className="timeline-meta">Session: 产品化</div>
          <p>核心引擎完成后，用TRAE快速搭建React+Vite的Playground界面、官网、主题系统和部署流程。</p>
          <ul>
            <li>用TRAE生成Playground布局、Monaco编辑器集成、SVG渲染逻辑</li>
            <li>让TRAE设计7套配色主题</li>
            <li>自动化部署脚本和Nginx配置</li>
          </ul>
        </div>
      </div>

      <h2>七条最佳实践</h2>
      <p>经过整个项目的实践，我们总结了在复杂算法项目中使用AI编程助手的七条经验：</p>
      {practices.map((practice, index) => (
        <div key={index} className="practice-card">
          <span className="num">{index + 1}</span>
          <div>
            <h4>{practice.title}</h4>
            <p>{practice.desc}</p>
          </div>
        </div>
      ))}

      <h2>提效数据</h2>
      <div className="metric-row">
        <div className="metric-card">
          <div className="value">70%+</div>
          <div className="label">AI 生成代码占比</div>
        </div>
        <div className="metric-card">
          <div className="value">3-4x</div>
          <div className="label">算法迭代速度提升</div>
        </div>
        <div className="metric-card">
          <div className="value">959</div>
          <div className="label">AI 辅助编写的测试用例</div>
        </div>
      </div>

      <div className="callout tip">
        <div className="callout-icon">💡</div>
        <div className="callout-body">
          <p><strong>关键结论：</strong>AI 编程助手不是「替代程序员」，而是「放大程序员」。一个有清晰架构思维和算法判断力的开发者，配合 AI 可以实现远超个人产能的项目。但如果缺乏方向感和质量把控，AI 也会快速产出大量垃圾代码。</p>
        </div>
      </div>

      <h2>继续探索</h2>
      <div className="quick-links">
        <a href="/" className="quick-link-card">
          <div className="ql-icon">🏠</div>
          <h4>返回首页</h4>
          <span className="ql-arrow">→</span>
        </a>
        <a href="/playground/" className="quick-link-card" target="_blank" rel="noopener noreferrer">
          <div className="ql-icon">🎮</div>
          <h4>体验 Playground</h4>
          <span className="ql-arrow">→</span>
        </a>
        <a href="/docs/how-it-works/" className="quick-link-card">
          <div className="ql-icon">🔧</div>
          <h4>技术揭秘</h4>
          <span className="ql-arrow">→</span>
        </a>
      </div>
    </DocPage>
  );
}
