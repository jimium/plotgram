import { useState } from 'react';
import DocPage, { DOCS_SIDEBAR } from '../components/DocPage';

const sidebar = DOCS_SIDEBAR.map((section) => ({
  ...section,
  items: section.items.map((item) => ({
    ...item,
    active: item.label === '路线图',
  })),
}));

interface RoadmapItem {
  phase: string;
  icon: string;
  title: string;
  status: string;
  desc: string;
  bullets: string[];
  demo?: string;
}

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
    phase: '远期',
    icon: '🌐',
    title: '平台集成',
    status: 'exploring',
    desc: 'Kroki 集成进入所有使用 Kroki 的平台；Markdown 代码块渲染插件（Docusaurus / MkDocs / remark）。',
    bullets: [
      'Kroki 支持 Plotgram 格式',
      'Docusaurus / MkDocs 插件',
      'remark / rehype 插件（适用 Next.js 等）',
      '````plotgram 代码块在静态站点中渲染',
    ],
  },
];

const STATUS_LABEL: Record<string, string> = {
  planning: '规划中',
  planned: '已排期',
  exploring: '探索中',
};

export default function Roadmap() {
  const [demoKey, setDemoKey] = useState(0);

  return (
    <DocPage
      title="🗺️ 路线图"
      description="Plotgram 正在演进的方向——从动画效果到 C4 架构图，从开发者工具链到平台生态。"
      sidebar={sidebar}
    >
      <div className="callout info">
        <div className="callout-icon">ℹ️</div>
        <div className="callout-body">
          <p>以下是 Plotgram 的演进方向，按近期 / 中期 / 远期划分。所有方向都围绕一个核心目标：<strong>让 Agent 更会画图，让一图胜千言的能力被 AI 放大</strong>。</p>
        </div>
      </div>

      {ROADMAP_ITEMS.map((item) => (
        <div key={item.title} className="roadmap-item">
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
                <span className="roadmap-demo-label">动画原型预览</span>
                <button
                  className="roadmap-demo-replay"
                  onClick={() => setDemoKey((k) => k + 1)}
                >
                  ↻ 重播
                </button>
              </div>
              <iframe
                key={demoKey}
                src={item.demo}
                className="roadmap-demo-iframe"
                title="动画效果演示"
              />
            </div>
          )}
        </div>
      ))}

      <div className="callout tip">
        <div className="callout-icon">💡</div>
        <div className="callout-body">
          <p><strong>优先级原则：</strong>近期聚焦"让图更好看、更会讲故事"（动画）和"覆盖更多企业场景"（C4）；中期补齐开发者工具链（MCP / VS Code / GitHub Action），让 Plotgram 进入日常工作流；远期做平台集成，扩大覆盖面。</p>
        </div>
      </div>
    </DocPage>
  );
}
