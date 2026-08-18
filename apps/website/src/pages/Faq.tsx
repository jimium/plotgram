import DocPage, { DOCS_SIDEBAR } from '../components/DocPage';

const sidebar = DOCS_SIDEBAR.map((section) => ({
  ...section,
  items: section.items.map((item) => ({
    ...item,
    active: item.label === '常见问题',
  })),
}));

export default function Faq() {
  return (
    <DocPage
      title="❓ 常见问题"
      description="关于 Tautcore 最常被问到的问题。"
      sidebar={sidebar}
    >
      <h2>基础问题</h2>
      <div className="faq-list">
        <div className="faq-item">
          <h3><span className="q">Q</span>Tautcore 和 Mermaid 有什么区别？为什么不直接在 Mermaid 基础上改进？</h3>
          <p>核心区别是设计目标不同。Mermaid 为人类手写设计，语法有大量变体和隐式规则，LLM 容易写错且出错后难以自修复；Tautcore 从零为 AI 生成设计——语法极简（只有 3 种箭头）、无隐式规则、结构化错误反馈（JSON 含行列位置和修复建议）、AST 一等公民支持增量修改。Mermaid 的语法包袱太重，在其基础上修修补补无法解决根本问题。</p>
        </div>
        <div className="faq-item">
          <h3><span className="q">Q</span>Tautcore 支持哪些图表类型？</h3>
          <p>目前稳定支持流程图（Flowchart）、时序图（Sequence）、架构图（Architecture）；Beta 支持状态机（State Machine）、ER 图、思维导图（Mindmap）。共 6 种类型，覆盖技术文档 90% 以上的使用场景。</p>
        </div>
        <div className="faq-item">
          <h3><span className="q">Q</span>可以免费使用吗？</h3>
          <p>可以。核心引擎完全开源，WASM 版本可以直接在浏览器中使用，Playground 永久免费。CLI 工具也可以自由使用。</p>
        </div>
        <div className="faq-item">
          <h3><span className="q">Q</span>支持 Mermaid 导入吗？</h3>
          <p>目前没有内置的 Mermaid 导入工具。Mermaid 的语法灵活性太高，自动转换很难保证输出质量。推荐参考「快速上手」页面的语法对照手动迁移，大部分图表 10 分钟内可以完成转换。</p>
        </div>
      </div>

      <h2>技术问题</h2>
      <div className="faq-list">
        <div className="faq-item">
          <h3><span className="q">Q</span>性能如何？能支持多大的图？</h3>
          <p>在普通笔记本浏览器中，200 节点的架构图渲染时间 &lt; 50ms；500 节点的极端场景约 200ms。CLI/WASM 使用相同的 Rust 核心，性能一致。超过 1000 节点的图建议分批渲染。</p>
        </div>
        <div className="faq-item">
          <h3><span className="q">Q</span>可以自托管吗？有 Docker 镜像吗？</h3>
          <p>可以。Tautcore 提供单二进制的 CLI 工具和 HTTP API 服务，部署非常简单。HTTP 服务只需要一个二进制文件，不依赖数据库或其他服务。</p>
        </div>
        <div className="faq-item">
          <h3><span className="q">Q</span>支持自定义主题和图标吗？</h3>
          <p>内置 7 套精心设计的主题（clean-light/clean-dark/blueprint/github-light/github-dark/okabe-ito/presentation）。自定义主题支持正在开发中；50+ 内置语义图标覆盖常见技术栈角色。</p>
        </div>
        <div className="faq-item">
          <h3><span className="q">Q</span>渲染结果是确定性的吗？同一输入会产生相同输出吗？</h3>
          <p>是的。所有分组排序操作使用 BTreeMap 和显式排序键，绝不依赖 HashMap 迭代顺序。同一输入多次渲染产生字节级相同的 SVG 输出，这对 snapshot 测试和 AI 增量修改至关重要。</p>
        </div>
      </div>

      <h2>AI/Agent 集成</h2>
      <div className="faq-list">
        <div className="faq-item">
          <h3><span className="q">Q</span>我的 LLM 应用如何集成 Tautcore？</h3>
          <p>有三种方式：1）直接调用 HTTP API（/render /validate）；2）在前端嵌入 WASM 包在浏览器中渲染；3）通过 CLI 在后端批处理。推荐阅读「AI Agent 集成指南」获取 Prompt 模板和错误处理最佳实践。</p>
        </div>
        <div className="faq-item">
          <h3><span className="q">Q</span>AI 生成的图有语法错误怎么办？</h3>
          <p>Tautcore 的 validate API 返回结构化 JSON 错误（含错误码、行列位置、上下文、修复建议）。Agent 可以将错误信息加入上下文中重试，实践中 90%+ 的错误一次重试即可修复。建议最多重试 2 次。</p>
        </div>
        <div className="faq-item">
          <h3><span className="q">Q</span>支持增量修改吗？不想每次都重新生成整张图。</h3>
          <p>支持。AST 可以序列化为 JSON，并提供语义级 Diff 和 Patch API。Agent 可以生成 Patch（如"把这个服务类型改成 database"、"加一条从 A 到 B 的边"）来增量修改图表，不需要重生成整张图。</p>
        </div>
      </div>

      <h2>项目与贡献</h2>
      <div className="faq-list">
        <div className="faq-item">
          <h3><span className="q">Q</span>这个项目用什么语言写的？</h3>
          <p>核心引擎用 Rust 编写（30,000+ 行），通过 wasm-pack 编译为 WASM 在浏览器中运行。CLI 和 HTTP 服务也使用 Rust。前端 Playground 和官网使用 React + TypeScript + Vite。</p>
        </div>
        <div className="faq-item">
          <h3><span className="q">Q</span>可以贡献代码吗？</h3>
          <p>项目计划开源，开源后欢迎贡献代码。目前可以在 Playground 中试用并反馈问题。</p>
        </div>
        <div className="faq-item">
          <h3><span className="q">Q</span>为什么叫 Tautcore？</h3>
          <p>Plot（绘图/情节）+ gram（书写/记录）。既表达了「绘制图表」的功能，也暗合「情节/逻辑的结构化表达」——Tautcore 的布局引擎不仅是画图形，更是在梳理和呈现逻辑关系。</p>
        </div>
      </div>
    </DocPage>
  );
}
