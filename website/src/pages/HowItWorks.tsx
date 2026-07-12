import DocPage, { DOCS_SIDEBAR } from '../components/DocPage';

const sidebar = DOCS_SIDEBAR.map((section) => ({
  ...section,
  items: section.items.map((item) => ({
    ...item,
    active: item.label === '技术揭秘',
  })),
}));

export default function HowItWorks() {
  return (
    <DocPage
      title="🔧 技术揭秘：Plotgram 渲染管线"
      description="深入了解 Plotgram 的布局引擎、边路由算法和质量保障体系。"
      sidebar={sidebar}
    >
      <div className="metric-row">
        <div className="metric-card">
          <div className="value">30,000+</div>
          <div className="label">Rust 核心代码行数</div>
        </div>
        <div className="metric-card">
          <div className="value">959</div>
          <div className="label">自动化测试用例</div>
        </div>
        <div className="metric-card">
          <div className="value">&lt;50ms</div>
          <div className="label">典型渲染耗时（200节点）</div>
        </div>
      </div>

      <h2>整体架构</h2>
      <p>Plotgram 采用 Rust 编写核心引擎，通过 WASM 编译在浏览器中运行，CLI/HTTP API/WASM 三端共享同一套核心代码。整体渲染管线分为 4 个阶段。</p>
      <div className="doc-feature-grid">
        <div className="doc-feature-card">
          <div className="icon">📝</div>
          <h4>阶段1: 语法解析</h4>
          <p>手写递归下降 Parser，生成强类型 AST</p>
        </div>
        <div className="doc-feature-card">
          <div className="icon">📐</div>
          <h4>阶段2: 布局计算</h4>
          <p>7种布局算法自动选择与执行</p>
        </div>
        <div className="doc-feature-card">
          <div className="icon">✏️</div>
          <h4>阶段3: 边路由</h4>
          <p>三层正交路由管线 + Edge Bundling</p>
        </div>
        <div className="doc-feature-card">
          <div className="icon">🎨</div>
          <h4>阶段4: SVG渲染</h4>
          <p>语义图标、主题系统、像素量化</p>
        </div>
      </div>

      <h2>布局算法体系</h2>
      <p>Plotgram 不是单一布局算法，而是根据图表类型自动选择最优布局策略的算法集合。</p>

      <div className="algo-card">
        <h3>Sugiyama-v2 增强分层布局 <span className="badge core">核心算法</span></h3>
        <p>针对经典层次流程图优化的 Sugiyama 方法，改进了坐标分配阶段以最小化边长方差和交叉数。支持分层约束、同层对齐和虚拟节点消除。</p>
      </div>

      <div className="algo-card">
        <h3>架构图双层布局 <span className="badge core">架构图专用</span></h3>
        <p>架构图需要处理分组嵌套、跨组边、兄弟组水平排列等特殊情况。双层布局先规划组边界和走廊，再在组内进行布局，保证架构图的结构清晰。</p>
      </div>

      <div className="algo-card">
        <h3>正交边路由三层管线 <span className="badge core">核心创新</span></h3>
        <p>Layer 1 通道规划 → Layer 2 边聚合（Edge Bundling）→ Layer 3 冲突重路由。三层管线确保即使是高密度架构图，边也不会混乱交叉。</p>
        <div className="algo-compare">
          <div className="before">
            <strong>优化前</strong>
            <p>边直接连线，密集区域形成"毛线球"，无法追踪数据流</p>
          </div>
          <div className="after">
            <strong>优化后</strong>
            <p>同向边聚合为共享trunk，平行边间距统一，入口/出口清晰</p>
          </div>
        </div>
      </div>

      <div className="algo-card">
        <h3>Edge Bundling 边聚合</h3>
        <p>识别平行/同向边段，将其聚合为共享trunk段，减少视觉ink用量30-50%。聚合后自动重算标签位置避免重叠，并设置最小ink节省阈值防止过度聚合。</p>
      </div>

      <div className="algo-card">
        <h3>像素量化与网格吸附 <span className="badge core">细节优化</span></h3>
        <p>自适应网格步长（&lt;20节点→4px, 20-50→8px, &gt;50→16px），使用Minimax位移选择最小化端点偏移，后处理简化近共线点。</p>
      </div>

      <h2>确定性渲染</h2>
      <p>为了避免图形抖动和测试不稳定，Plotgram 在所有分组和排序操作中严格使用显式排序键（BTreeMap/按ID排序），绝不依赖HashMap的迭代顺序。这确保：</p>
      <ul>
        <li>同一输入多次渲染产生完全一致的输出</li>
        <li>Snapshot测试可靠</li>
        <li>AI修改局部不会引起全局布局抖动</li>
        <li>边端口选择稳定可预测</li>
      </ul>

      <h2>质量保障体系</h2>
      <div className="doc-feature-grid">
        <div className="doc-feature-card">
          <div className="icon">✅</div>
          <h4>959个Snapshot测试</h4>
          <p>覆盖所有6种图表类型和边界情况</p>
        </div>
        <div className="doc-feature-card">
          <div className="icon">📊</div>
          <h4>算法Benchmark</h4>
          <p>量化ink用量、交叉数、边长方差</p>
        </div>
        <div className="doc-feature-card">
          <div className="icon">🔄</div>
          <h4>多轮冲突解决</h4>
          <p>软惩罚→硬障碍→nudge微调，最多3轮</p>
        </div>
        <div className="doc-feature-card">
          <div className="icon">🛡️</div>
          <h4>安全回退</h4>
          <p>边聚合或重路由失败时回退到保守路径</p>
        </div>
      </div>

      <h2>技术栈</h2>
      <table>
        <thead>
          <tr>
            <th>层</th>
            <th>技术</th>
            <th>用途</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <td>核心引擎</td>
            <td>Rust + no_std compatible</td>
            <td>解析/布局/路由/渲染核心</td>
          </tr>
          <tr>
            <td>浏览器</td>
            <td>Rust → WASM</td>
            <td>在浏览器中直接渲染，无需服务端</td>
          </tr>
          <tr>
            <td>CLI</td>
            <td>Rust 二进制</td>
            <td>本地渲染/批量处理/CI集成</td>
          </tr>
          <tr>
            <td>HTTP服务</td>
            <td>Axum (Rust)</td>
            <td>提供REST API服务</td>
          </tr>
          <tr>
            <td>Web前端</td>
            <td>React + Vite + TypeScript</td>
            <td>Playground 和官网</td>
          </tr>
          <tr>
            <td>语法设计</td>
            <td>自定义DSL</td>
            <td>AI友好的极简语法规范</td>
          </tr>
        </tbody>
      </table>

      <h2>了解更多</h2>
      <div className="quick-links">
        <a href="/docs/agent-guide/" className="quick-link-card">
          <div className="ql-icon">🤖</div>
          <h4>AI Agent 集成</h4>
          <p>将 Plotgram 嵌入 LLM 应用和 AI Agent</p>
          <span className="ql-arrow">→</span>
        </a>
        <a href="/docs/trae-story/" className="quick-link-card">
          <div className="ql-icon">📖</div>
          <h4>TRAE 开发实践</h4>
          <p>了解 Plotgram 的开发历程和经验</p>
          <span className="ql-arrow">→</span>
        </a>
        <a href="/playground/" className="quick-link-card" target="_blank" rel="noopener noreferrer">
          <div className="ql-icon">🎮</div>
          <h4>立即试用 Playground</h4>
          <p>在线体验 Plotgram 图表渲染</p>
          <span className="ql-arrow">→</span>
        </a>
      </div>
    </DocPage>
  );
}
